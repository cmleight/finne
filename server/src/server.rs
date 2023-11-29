use crate::request::{CreateRequest, Request};
use futures::task;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};
use rusqlite::Connection;
use slab::Slab;
use std::collections::VecDeque;
use std::future::Future;
use std::io::{Read, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::time::Duration;
use bytes::BufMut;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use finne_parser::request_parser::Method;

static OK: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static SERVER_ERROR: &[u8] = b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static MISSING: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";

pub struct Search {
    future:Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
    executor: mpsc::Sender<Arc<Search>>
}

impl Search {
    async fn schedule(self: &Arc<Self>) {
        self.executor.send(self.clone()).await;
    }
}

pub struct Server {
    scratch: [u8; 1024],
    poll: Poll,
    events: Events,
    listener: TcpListener,
    sockets: Slab<Reusable<'static, Request>>,
    db_conn: Connection,
    buf_pool: Pool<Request>,
    tasks: VecDeque<Pin<Box<JoinHandle<()>>>>,
    tx: mpsc::Sender<()>,
    rx: mpsc::Receiver<()>,
}

impl Server {
    pub fn new(address: Option<String>, db_conn: Connection) -> Self {
        let mut poll = Poll::new().unwrap();
        let mut listener = TcpListener::bind(
            address
                .unwrap_or("0.0.0.0:3000".to_string())
                .parse()
                .unwrap(),
        )
        .unwrap();
        poll.registry()
            .register(&mut listener, Token(0), Interest::READABLE)
            .unwrap();
        let events = Events::with_capacity(1024);
        let (tx, mut rx) = mpsc::channel(1024);
        Self {
            scratch: [0_u8; 1024],
            poll,
            events,
            listener,
            sockets: Slab::new(),
            db_conn,
            buf_pool: Pool::new(300, &|| Request::default()),
            tasks: Default::default(),
            tx,
            rx,
        }
    }

    #[inline(always)]
    fn pull_or_create(self) -> Reusable<'static, Request> {
        return self.buf_pool.pull(|| {
            println!("Miss object pool allocation!");
            return Request::default();
        });
    }

    pub async fn run(mut self) {
        let waker = task::waker(Arc);
        let mut cx = Context::from_waker(&waker);
        loop {
            self.poll
                .poll(&mut self.events, Some(Duration::from_secs(0)))
                .unwrap();
            for event in self.events.iter() {
                match event.token() {
                    Token(0) => loop {
                        match self.listener.accept() {
                            Ok((mut socket, _)) => {
                                let next = self.sockets.vacant_entry();
                                self.poll
                                    .registry()
                                    .register(
                                        &mut socket,
                                        Token(next.key() + 1),
                                        Interest::READABLE,
                                    )
                                    .unwrap();
                                let mut request = self.pull_or_create();
                                request.set_socket(socket);
                                next.insert(request);
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    },
                    token if event.is_readable() => {
                        let request_number = token.0 - 1;
                        let mut task = Box::pin(tokio::spawn(async move {
                            let conn = self.sockets.get_mut(request_number);
                            if !self.receive_request(
                                conn.unwrap(),
                                request_number,
                            ) {
                                self.sockets.remove(request_number);
                            }
                        }));
                        if task.as_mut().poll(&mut cx).is_pending() {

                        } else {
                            self.tasks.push_back(task);
                        }
                    }
                    token if event.is_writable() => {
                        let conn = self.sockets.get_mut(token.0 - 1).unwrap();
                        self.write_response(token, conn);
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn receive_request(
        mut self,
        buffers: &mut Request,
        token: usize,
    ) -> bool {
        buffers.clear();
        loop {
            let read = buffers.socket.unwrap().read(&mut self.scratch);
            match read {
                Ok(0) => {
                    return true;
                }
                Ok(n) => buffers.parse_buf.put(&self.scratch[0..n]),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        self.process_request(buffers);
        self.poll.registry()
            .reregister(&mut buffers.socket.unwrap(), Token(token + 1), Interest::WRITABLE)
            .unwrap();
        return false;
    }
    pub fn write_response(mut self, token: Token, buffers: &mut Request) {
        let resp = &buffers.resp_buf;
        buffers.socket.unwrap().write_all(resp).unwrap();

        self.poll
            .registry()
            .reregister(&mut buffers.socket.unwrap(), token, Interest::READABLE)
            .unwrap();
    }

    fn process_request(mut self, buffers: &mut Request) {
        let http_req = match finne_parser::request_parser::parse_request(&mut buffers.parse_buf) {
            Ok(req) => req,
            Err(e) => {
                println!("Error parsing request: {:?}", e);
                println!("Request: {:?}", buffers.parse_buf);
                return;
            }
        };
        let (status_code, body): (&[u8], &str) =
            match (http_req.path, http_req.method, http_req.body) {
                (b"/", Method::Get, _) => (OK, "index\n"),
                (b"/c" | b"/create", Method::Post, req_body) => match create(&mut self.db_conn, req_body) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                (b"/u" | b"/update", Method::Post | Method::Put, _req_body) => match update(&mut self.db_conn) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                (b"/d" | b"/delete", Method::Delete, _) => match delete(&mut self.db_conn) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                (b"/s" | b"/search", Method::Get, _) => match search(self.db_conn) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                _ => (MISSING, "404\n"),
            };

        buffers.create_html_response(status_code, body.as_bytes());
    }
}

pub enum Error {
    InvalidRequest,
    _Data,
}

#[inline]
fn create(_conn: &mut Connection, body: &[u8]) -> Result<bool, Error> {
    match serde_json::from_slice::<CreateRequest>(body) {
        Ok(_req) => {
            return Ok(true);
        }
        Err(e) => {
            println!("Error parsing request: {:?}", e);
            println!("Request: {:?}", body);
            return Err(Error::InvalidRequest);
        }
    };
}

#[inline]
fn update(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn search(_conn: Connection) -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn delete(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}

