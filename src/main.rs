use std::io::{Read, Write};
use std::ops::DerefMut;
use std::time::Duration;

use bytes::{Buf, BufMut, BytesMut};
use clap::Parser;
use httparse;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};
use rusqlite::Connection;
use slab::Slab;

mod request_parser;

const BUF_EXPANSION: usize = 1024;

#[derive(Parser)]
struct Cli {
    path: std::path::PathBuf,
}

struct RequestBuffers {
    parse_buf: BytesMut,
    resp_buf: BytesMut,
}

impl RequestBuffers {
    #[inline(always)]
    fn clear(&mut self) {
        self.parse_buf.clear();
        self.resp_buf.clear();
    }
}

impl Default for RequestBuffers {
    #[inline(always)]
    fn default() -> Self {
        return RequestBuffers {
            parse_buf: BytesMut::new(),
            resp_buf: BytesMut::new(),
        };
    }
}

struct ConnectionData<'a> {
    socket: TcpStream,
    buffers: Reusable<'a, RequestBuffers>,
}

#[inline(always)]
fn pull_or_create<'a>(pool: &'a Pool<RequestBuffers>) -> Reusable<'a, RequestBuffers> {
    return pool.pull(|| {
        println!("Miss object pool allocation!");
        return RequestBuffers::default();
    });
}

fn main() {
    let args = Cli::parse();
    let mut db_conn = match Connection::open(args.path) {
        Ok(conn) => conn,
        Err(e) => panic!("Encountered error {:?}", e),
    };
    let address = "0.0.0.0:3000";
    let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    let buf_pool = Pool::new(300, &|| RequestBuffers::default());

    let mut events = Events::with_capacity(BUF_EXPANSION);
    let mut buffer = [0_u8; BUF_EXPANSION];
    {
        let mut sockets: Slab<ConnectionData> = Slab::new();
        loop {
            poll.poll(&mut events, Some(Duration::from_secs(0)))
                .unwrap();

            // Get requests
            for event in &events {
                match event.token() {
                    Token(0) => loop {
                        match listener.accept() {
                            Ok((mut socket, _)) => {
                                let next = sockets.vacant_entry();
                                poll.registry()
                                    .register(
                                        &mut socket,
                                        Token(next.key() + 1),
                                        Interest::READABLE,
                                    )
                                    .unwrap();
                                next.insert(ConnectionData {
                                    socket,
                                    buffers: pull_or_create(&buf_pool),
                                });
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    },
                    token if event.is_readable() => {
                        let request_number = token.0 - 1;
                        receive_request(
                            request_number,
                            &mut sockets,
                            &mut poll,
                            &mut buffer,
                            &mut db_conn,
                        );
                        // pending_requests.push(request_number);
                    }
                    token if event.is_writable() => {
                        let socket = sockets.get_mut(token.0 - 1).unwrap();
                        let resp = &socket.buffers.resp_buf;
                        socket.socket.write_all(resp).unwrap();

                        poll.registry()
                            .reregister(&mut socket.socket, token, Interest::READABLE)
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

fn receive_request(
    token: usize,
    sockets: &mut Slab<ConnectionData>,
    poll: &mut Poll,
    buffer: &mut [u8],
    db_conn: &mut Connection,
) {
    let conn = sockets.get_mut(token).unwrap();
    conn.buffers.clear();
    loop {
        let read = conn.socket.read(buffer);
        match read {
            Ok(0) => {
                sockets.remove(token);
                break;
            }
            Ok(n) => conn.buffers.parse_buf.put(&buffer[0..n]),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }

    if let Some(conn) = sockets.get_mut(token) {
        process_request(db_conn, conn.buffers.deref_mut());
        poll.registry()
            .reregister(&mut conn.socket, Token(token + 1), Interest::WRITABLE)
            .unwrap();
    }
}

fn process_request(conn: &mut Connection, req: &mut RequestBuffers) {
    let mut headers = [httparse::Header {
        name: "",
        value: &[],
    }; 16];
    let mut request = httparse::Request::new(&mut headers);
    if request.parse(req.parse_buf.chunk()).unwrap().is_complete() {
        let url = request.path.unwrap_or("404");
        let path = url.find("?").unwrap_or(url.len());
        let (status_code, body): (&[u8], &str) = match (&url[..path], request.method) {
            ("/", Some("GET")) => (OK, "index\n"),
            ("/c", Some("POST")) | ("/create", Some("POST")) => match create(conn) {
                Ok(_) => (OK, "create\n"),
                Err(_) => (SERVER_ERROR, "create\n"),
            },
            ("/u", Some("POST"))
            | ("/u", Some("PUT"))
            | ("/update", Some("POST"))
            | ("/update", Some("PUT")) => match update(conn) {
                Ok(_) => (OK, "search\n"),
                Err(_) => (SERVER_ERROR, "search\n"),
            },
            ("/d", Some("DELETE")) | ("/delete", Some("DELETE")) => match delete(conn) {
                Ok(_) => (OK, "search\n"),
                Err(_) => (SERVER_ERROR, "search\n"),
            },
            ("/s", Some("GET")) | ("/search", Some("GET")) => match search(conn) {
                Ok(_) => (OK, "search\n"),
                Err(_) => (SERVER_ERROR, "search\n"),
            },
            _ => (MISSING, "404\n"),
        };

        create_html_response(&mut req.resp_buf, status_code, body.as_bytes());
    }
}

static OK: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static SERVER_ERROR: &[u8] = b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static MISSING: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static BODY_DELIM: &[u8] = b"\r\n\r\n";

#[inline]
fn create_html_response(resp_buf: &mut BytesMut, status_code: &[u8], body: &[u8]) {
    resp_buf.put_slice(status_code);
    resp_buf.put_slice(body.len().to_string().as_bytes());
    resp_buf.put_slice(BODY_DELIM);
    resp_buf.put_slice(body);
}

enum _Operation {
    Update,
    Delete,
    Query,
}

enum Error {
    _InvalidRequest,
    _Data,
}

#[inline]
fn create(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn update(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn search(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn delete(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}
