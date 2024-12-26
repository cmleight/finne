use std::collections::HashMap;
use std::io;
use std::io::{Read, Write};
use std::ops::DerefMut;
use std::time::Duration;

use bytes::{BufMut, BytesMut};
use clap::Parser;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};
use serde::Deserialize;
use serde_json;
use slab::Slab;

// use finne_parser::request_parser::HttpRequest;
use finne_parser::request_parser::Method;

const BUF_EXPANSION: usize = 1024;

#[derive(Parser)]
struct Cli {
    #[arg(default_value="./test.db")]
    path: std::path::PathBuf,
    #[arg(default_value = "8080")]
    port: String,
    #[arg(default_value = "3000")]
    management_port: String,
}

struct RequestBuffers {
    parse_buf: BytesMut,
    resp_buf: BytesMut,
    is_management: bool,
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
            is_management: false,
        };
    }
}

struct ConnectionData<'a> {
    socket: TcpStream,
    buffers: Reusable<'a, RequestBuffers>,
}

#[inline(always)]
fn pull_or_create(pool: &Pool<RequestBuffers>, is_management: bool) -> Reusable<RequestBuffers> {
    let mut buf =  pool.pull(|| {
        println!("Miss object pool allocation!");
        return RequestBuffers::default();
    });
    buf.is_management = is_management;
    return buf;
}

const SERVER: Token = Token(0);
const MANAGER: Token = Token(1);
// Adjust the token offset by one greater than the number of tokens that we have.
const MAX_TOKEN: usize = 2;

fn main() -> io::Result<()> {
    let args = Cli::parse();
    // setup listeners
    let mut server_listener = TcpListener::bind((
        "0.0.0.0:".to_owned() + &args.port
    ).parse().unwrap())?;
    let mut management_listener = TcpListener::bind((
        "0.0.0.0:".to_owned() + &args.management_port
    ).parse().unwrap())?;

    // create poll and register listeners
    let mut poll = Poll::new()?;
    poll.registry()
        .register(&mut server_listener, SERVER, Interest::READABLE)?;
    poll.registry()
        .register(&mut management_listener, MANAGER, Interest::READABLE)?;

    let buf_pool = Pool::new(300, &|| RequestBuffers::default());

    let mut events = Events::with_capacity(BUF_EXPANSION);
    let mut buffer = [0_u8; BUF_EXPANSION];
    {
        let mut sockets: Slab<ConnectionData> = Slab::new();
        loop {
            if let Err(err) = poll.poll(&mut events, Some(Duration::from_secs(0))) {
                if interrupted(&err) {
                    continue;
                }
                return Err(err);
            }

            // Get requests
            for event in events.iter() {
                println!("{:?}", event);
                match event.token() {
                    SERVER => loop {
                        match server_listener.accept() {
                            Ok((mut socket, _)) => {
                                let next = sockets.vacant_entry();
                                poll.registry()
                                    .register(
                                        &mut socket,
                                        Token(next.key() + MAX_TOKEN),
                                        Interest::READABLE,
                                    )?;
                                next.insert(ConnectionData {
                                    socket,
                                    buffers: pull_or_create(&buf_pool, false),
                                });
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    },
                    MANAGER => loop {
                        match management_listener.accept() {
                            Ok((mut socket, _)) => {
                                let next = sockets.vacant_entry();
                                poll.registry()
                                    .register(
                                        &mut socket,
                                        Token(next.key() + MAX_TOKEN),
                                        Interest::READABLE,
                                    )?;
                                next.insert(ConnectionData {
                                    socket,
                                    buffers: pull_or_create(&buf_pool, true),
                                });
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    }
                    token if event.is_readable() => {
                        let request_number = token.0 - MAX_TOKEN;
                        receive_request(
                            request_number,
                            &mut sockets,
                            &mut poll,
                            &mut buffer,
                        );
                        // pending_requests.push(request_number);
                    }
                    token if event.is_writable() => {
                        let socket = sockets.get_mut(token.0 - MAX_TOKEN).unwrap();
                        let resp = &socket.buffers.resp_buf;
                        socket.socket.write_all(resp)?;

                        poll.registry()
                            .reregister(&mut socket.socket, token, Interest::READABLE)?;
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}

fn receive_request(
    token: usize,
    sockets: &mut Slab<ConnectionData>,
    poll: &mut Poll,
    buffer: &mut [u8],
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
        process_request(conn.buffers.deref_mut());
        poll.registry()
            .reregister(&mut conn.socket, Token(token + 2), Interest::WRITABLE)
            .unwrap();
    }
}

fn process_request(req: &mut RequestBuffers) {
    let http_req = match finne_parser::request_parser::parse_request(&req.parse_buf) {
        Ok(req) => req,
        Err(e) => {
            println!("Error parsing request: {:?}", e);
            println!("Request: {:?}", req.parse_buf);
            return;
        }
    };
    let (status_code, body): (&[u8], &str) = match (http_req.path, http_req.method, http_req.body) {
        (b"/", Method::Get, _) => (OK, "index\n"),
        (b"/c" | b"/create", Method::Post, req_body) => match create(req_body) {
            Ok(_) => (OK, "search\n"),
            Err(_) => (SERVER_ERROR, "search\n"),
        },
        (b"/u" | b"/update", Method::Post | Method::Put, _req_body) => match update() {
            Ok(_) => (OK, "search\n"),
            Err(_) => (SERVER_ERROR, "search\n"),
        },
        (b"/d" | b"/delete", Method::Delete, _) => match delete() {
            Ok(_) => (OK, "search\n"),
            Err(_) => (SERVER_ERROR, "search\n"),
        },
        (b"/s" | b"/search", Method::Get, _) => match search() {
            Ok(_) => (OK, "search\n"),
            Err(_) => (SERVER_ERROR, "search\n"),
        },
        _ => (MISSING, "404\n"),
    };

    create_html_response(&mut req.resp_buf, status_code, body.as_bytes());
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

enum Error {
    InvalidRequest,
    _Data,
}

#[derive(Deserialize)]
enum IndexType {
    Integer,
    Real,
    Text,
}

#[derive(Deserialize)]
struct CreateRequest {
    _name: String,
    _indexes: HashMap<String, IndexType>,
}

#[inline]
fn create(body: &[u8]) -> Result<bool, Error> {
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
fn update() -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn search() -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn delete() -> Result<bool, Error> {
    return Ok(true);
}
