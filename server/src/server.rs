use crate::request::{ConnectionData, RequestBuffers};
use futures::task;
use mio::net::TcpListener;
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};
use rusqlite::Connection;
use slab::Slab;
use std::collections::VecDeque;
use std::future::Future;
use std::task::Context;
use std::time::Duration;
use tokio::sync::mpsc;

pub struct Server {
    poll: Poll,
    events: Events,
    listener: TcpListener,
    sockets: Slab<ConnectionData<'static>>,
    db_conn: Connection,
    buf_pool: Pool<RequestBuffers>,
    tasks: VecDeque<task::Waker>,
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
            poll,
            events,
            listener,
            sockets: Slab::new(),
            db_conn,
            buf_pool: Pool::new(300, &|| RequestBuffers::default()),
            tasks: Default::default(),
            tx,

            rx,
        }
    }

    #[inline(always)]
    fn pull_or_create(self) -> Reusable<'static, RequestBuffers> {
        return self.buf_pool.pull(|| {
            println!("Miss object pool allocation!");
            return RequestBuffers::default();
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
                                next.insert(ConnectionData::new(socket, self.pull_or_create()));
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    },
                    token if event.is_readable() => {
                        let request_number = token.0 - 1;
                        let mut task = Box::pin(tokio::spawn(async move {
                            let conn = self.sockets.get_mut(request_number);
                            if !conn.unwrap().receive_request(
                                request_number,
                                &mut self.poll,
                                &mut self.db_conn,
                            ) {
                                self.sockets.remove(request_number);
                            }
                        }));
                        if task.as_mut().poll(&mut cx).is_pending() {

                        } else {
                            self.tasks.push(task);
                        }
                    }
                    token if event.is_writable() => {
                        let socket = self.sockets.get_mut(token.0 - 1).unwrap();
                        socket.write_response(self.poll, token)
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}
