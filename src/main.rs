use std::collections::HashMap;
use std::io::{Read, Write};
use std::ops::DerefMut;
use std::time::{Duration};

use bytes::{BufMut, BytesMut};
use clap::Parser;
use httparse;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};
use slab::Slab;

// use rusqlite::Connection;

#[derive(Parser)]
struct Cli {
    path: std::path::PathBuf,
}

struct RequestBuffers<'a> {
    parse_buf: BytesMut,
    resp_buf: BytesMut,
    params: HashMap<&'a str, &'a str>,
}

impl RequestBuffers<'_> {
    #[inline(always)]
    fn clear(&mut self) {
        self.parse_buf.clear();
        self.resp_buf.clear();
        self.params.clear();
    }
}

impl Default for RequestBuffers<'_> {
    #[inline(always)]
    fn default() -> Self {
        return RequestBuffers {
            parse_buf: BytesMut::new(),
            resp_buf: BytesMut::new(),
            params: HashMap::new(),
        }
    }
}

struct ConnectionData<'a> {
    socket: TcpStream,
    buffers: Reusable<'a, RequestBuffers<'a>>,
}

#[inline(always)]
fn pull_or_create<'a>(pool: &'a Pool<RequestBuffers<'a>>) -> Reusable<'a, RequestBuffers<'a>> {
    return pool.pull(|| {
        println!("Miss object pool allocation!");
        return RequestBuffers::default();
    });
}

fn main() {
    // let args = Cli::parse();
    // let db_conn = Connection::open(args.path);
    let address = "0.0.0.0:3000";
    let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    let buf_pool = Pool::new(300, &|| RequestBuffers::default());

    let mut events = Events::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    {
        let mut sockets: Slab<ConnectionData> = Slab::new();
        loop {
            poll.poll(&mut events, Some(Duration::from_secs(0))).unwrap();

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
                            },
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    },
                    token if event.is_readable() => {
                        let request_number = token.0 - 1;
                        receive_request(request_number, &mut sockets, &mut poll, &mut buffer);
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
        process_request(&mut conn.buffers.deref_mut());
        poll.registry()
            .reregister(&mut conn.socket, Token(token + 1), Interest::WRITABLE)
            .unwrap();
    }
}

fn process_request(req: &mut RequestBuffers) {
    let mut headers = [httparse::Header {
        name: "",
        value: &[],
    }; 16];
    let mut request = httparse::Request::new(&mut headers);
    if request
        .parse(&mut req.parse_buf)
        .unwrap()
        .is_complete()
    {
        // println!("Body: {:?}", request);
        let url = request.path.unwrap_or("404");
        let path = url.find("?").unwrap_or(url.len());
        let (status_code, body): (&[u8], &str) = match (&url[..path], request.method) {
            ("/", Some("GET")) => (OK, "index\n"),
            ("/u", Some("POST"))
            | ("/u", Some("PUT"))
            | ("/update", Some("POST"))
            | ("/update", Some("PUT")) => match update() {
                Ok(_) => (OK, "search\n"),
                Err(_) => (SERVER_ERROR, "search\n"),
            },
            ("/d", Some("DELETE")) | ("/delete", Some("DELETE")) => match delete() {
                Ok(_) => (OK, "search\n"),
                Err(_) => (SERVER_ERROR, "search\n"),
            },
            ("/s", Some("GET"))
            | ("/search", Some("GET")) => match search() {
                Ok(_) => (OK, "search\n"),
                Err(_) => (SERVER_ERROR, "search\n"),
            },
            _ => (MISSING, "404\n"),
        };

        create_html_response(&mut req.resp_buf, status_code, body.as_bytes());
    }
}

static OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static SERVER_ERROR: &[u8] = b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static MISSING: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";

// fn parse_params() ->

#[inline]
fn create_html_response(resp_buf: &mut BytesMut, status_code: &[u8], body: &[u8]) {
    resp_buf.put_slice(status_code);
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

#[inline]
fn parse_params<'a>(url_params: &'a str, params: &mut HashMap<&'a str, &'a str>) {
    return url_params
        .split("&")
        .filter_map(|param| param.split_once("="))
        .for_each(|(key, value)| _ = params.insert(key, value));
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
