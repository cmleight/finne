use bytes::{BufMut, BytesMut};
use finne_parser::request_parser::Method;
use mio::net::TcpStream;
use mio::{Interest, Poll, Token};
use object_pool::Reusable;
use rusqlite::Connection;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};

static OK: &[u8] =
    b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static SERVER_ERROR: &[u8] = b"HTTP/1.1 500 Internal Server Error\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static MISSING: &[u8] = b"HTTP/1.1 404 Not Found\r\nContent-Type: text/html\r\nConnection: keep-alive\r\nContent-Length: ";
static BODY_DELIM: &[u8] = b"\r\n\r\n";
pub enum Error {
    InvalidRequest,
    _Data,
}

#[derive(Deserialize)]
pub enum IndexType {
    Integer,
    Real,
    Text,
}

#[derive(Deserialize)]
pub struct CreateRequest {
    _name: String,
    _indexes: HashMap<String, IndexType>,
}

pub struct RequestBuffers {
    pub(crate) parse_buf: BytesMut,
    pub(crate) resp_buf: BytesMut,
}

impl RequestBuffers {
    #[inline(always)]
    pub fn clear(&mut self) {
        self.parse_buf.clear();
        self.resp_buf.clear();
    }
}

pub struct ConnectionData<'a> {
    socket: TcpStream,
    buffers: Reusable<'a, RequestBuffers>,
    scratch: [u8; 1024],
}

impl ConnectionData<'_> {
    pub fn new<'a>(socket: TcpStream, buffers: Reusable<RequestBuffers>) -> ConnectionData<'a> {
        Self {
            socket,
            buffers,
            scratch: [0_u8; 1024],
        }
    }

    pub fn write_response(mut self, poll: Poll, token: Token) {
        let resp = &self.buffers.resp_buf;
        self.socket.write_all(resp).unwrap();

        poll
            .registry()
            .reregister(&mut self.socket, token, Interest::READABLE)
            .unwrap();
    }

    pub fn receive_request(
        mut self,
        token: usize,
        poll: &mut Poll,
        db_conn: &mut Connection,
    ) -> bool {
        self.buffers.clear();
        loop {
            let read = self.socket.read(&mut *self.scratch);
            match read {
                Ok(0) => {
                    return true;
                }
                Ok(n) => self.buffers.parse_buf.put(self.scratch[0..n]),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        self.process_request(db_conn);
        poll.registry()
            .reregister(&mut self.socket, Token(token + 1), Interest::WRITABLE)
            .unwrap();
        return false;
    }

    pub(crate) fn process_request(self, conn: &mut Connection) {
        let http_req = match finne_parser::request_parser::parse_request(&self.buffers.parse_buf) {
            Ok(req) => req,
            Err(e) => {
                println!("Error parsing request: {:?}", e);
                println!("Request: {:?}", self.buffers.parse_buf);
                return;
            }
        };
        let (status_code, body): (&[u8], &str) =
            match (http_req.path, http_req.method, http_req.body) {
                (b"/", Method::Get, _) => (OK, "index\n"),
                (b"/c" | b"/create", Method::Post, req_body) => match create(conn, req_body) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                (b"/u" | b"/update", Method::Post | Method::Put, _req_body) => match update(conn) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                (b"/d" | b"/delete", Method::Delete, _) => match delete(conn) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                (b"/s" | b"/search", Method::Get, _) => match search(conn) {
                    Ok(_) => (OK, "search\n"),
                    Err(_) => (SERVER_ERROR, "search\n"),
                },
                _ => (MISSING, "404\n"),
            };

        self.create_html_response(status_code, body.as_bytes());
    }
    #[inline]
    fn create_html_response(self, status_code: &[u8], body: &[u8]) {
        self.buffers.resp_buf.put_slice(status_code);
        self.buffers
            .resp_buf
            .put_slice(body.len().to_string().as_bytes());
        self.buffers.resp_buf.put_slice(BODY_DELIM);
        self.buffers.resp_buf.put_slice(body);
    }
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
fn search(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}

#[inline]
fn delete(_conn: &mut Connection) -> Result<bool, Error> {
    return Ok(true);
}
