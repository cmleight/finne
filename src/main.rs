use std::future::pending;
use std::io::{Read, Write};
use std::ops::DerefMut;
use std::time::{Duration, Instant};

use bytes::{BufMut, BytesMut};
use clap::Parser;
use httparse;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};
use slab::Slab;

use rusqlite::Connection;

#[derive(Parser)]
struct Cli {
    path: std::path::PathBuf,
}

struct RequestBuffers {
    parse_buf: BytesMut,
    resp_buf: BytesMut,
}

impl RequestBuffers {
    fn clear(&mut self) {
        self.parse_buf.clear();
        self.resp_buf.clear();
    }
}

struct ConnectionData<'a> {
    socket: TcpStream,
    buffers: Reusable<'a, RequestBuffers>,
}

#[inline]
fn pull_or_create(pool: &Pool<RequestBuffers>) -> Reusable<RequestBuffers> {
    return pool.pull(|| {
        println!("Miss object pool allocation!");
        return RequestBuffers {
            parse_buf: BytesMut::with_capacity(1024),
            resp_buf: BytesMut::with_capacity(1024),
        };
    });
}

fn main() {
    let args = Cli::parse();
    let db_conn = Connection::open(args.path);
    let address = "0.0.0.0:3000";
    let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    let buf_pool = Pool::new(300, &|| RequestBuffers {
        parse_buf: BytesMut::with_capacity(1024),
        resp_buf: BytesMut::with_capacity(1024),
    });

    let mut pending_requests: Vec<usize> = Vec::new();
    let mut events = Events::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    {
        let mut sockets: Slab<ConnectionData> = Slab::new();
        loop {
            let poll_duration = if pending_requests.len() == 0 {
                None
            } else {
                Some(Duration::from_secs(0))
            };
            poll.poll(&mut events, poll_duration).unwrap();

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
                        receive_request(request_number, &mut sockets, &mut poll, &mut buffer);
                        pending_requests.push(request_number);
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

            // Process requests
            let now = Instant::now();
            while now - Instant::now() < Duration::from_millis(100) && pending_requests.len() > 0 {
                let curr_req = pending_requests.pop();
                println!("Processing {:?}", curr_req);
            }
        }
    }
}

fn receive_request(
    token: usize,
    sockets: &mut Slab<ConnectionData>,
    poll: &mut Poll,
    buffer: &mut [u8],
) {
    let mut headers = [httparse::Header {
        name: "",
        value: &[],
    }; 16];
    let mut request_parser = httparse::Request::new(&mut headers);
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
        let buffers = conn.buffers.deref_mut();
        if request_parser
            .parse(&mut buffers.parse_buf)
            .unwrap()
            .is_complete()
        {
            process_request(request_parser, &mut buffers.resp_buf);
            poll.registry()
                .reregister(&mut conn.socket, Token(token + 1), Interest::WRITABLE)
                .unwrap();
        }
    }
}

fn process_request(request: httparse::Request, resp_buf: &mut BytesMut) {
    let (status_code, body): (&[u8], &str) = match (request.path, request.method) {
        (Some("/"), Some("GET")) => (OK, "index\n"),
        (Some("/u"), Some("POST"))
        | (Some("/u"), Some("PUT"))
        | (Some("/update"), Some("POST"))
        | (Some("/update"), Some("PUT")) => match update() {
            Ok(_) => (OK, "search\n"),
            Err(_) => (SERVER_ERROR, "search\n"),
        },
        (Some("/d"), Some("DELETE")) | (Some("/delete"), Some("DELETE")) => match update() {
            Ok(_) => (OK, "search\n"),
            Err(_) => (SERVER_ERROR, "search\n"),
        },
        (Some("/s"), Some("GET"))
        | (Some("/search"), Some("GET"))
        | (Some("/q"), Some("GET"))
        | (Some("/query"), Some("GET")) => match update() {
            Ok(_) => (OK, "search\n"),
            Err(_) => (SERVER_ERROR, "search\n"),
        },
        _ => (b"404 Not Found", "\n"),
    };

    create_html_response(resp_buf, status_code, body.as_bytes());
}

static OK: &[u8] = b"200 OK";
static SERVER_ERROR: &[u8] = b"500 Internal Server Error";
static CONNECTION_CONTENT: &[u8] =
    b"\nContent-Type: text/html\nConnection: keep-alive\nContent-Length: ";
static HTML_VERSION: &[u8] = b"HTTP/1.1 \n";

fn create_html_response(resp_buf: &mut BytesMut, status_code: &[u8], body: &[u8]) {
    resp_buf.put_slice(HTML_VERSION);
    resp_buf.put_slice(status_code);
    resp_buf.put_slice(CONNECTION_CONTENT);
    resp_buf.put_slice(body.len().to_string().as_bytes());
    resp_buf.put_slice(b"\r\n\r\n");
    resp_buf.put_slice(body);
}

enum Operation {
    Update,
    Delete,
    Query,
}

enum Error {
    InvalidRequest,
    Data,
}

fn update() -> Result<bool, Error> {
    return Ok(true);
}

fn query() -> Result<bool, Error> {
    return Ok(true);
}

fn delete() -> Result<bool, Error> {
    return Ok(true);
}
