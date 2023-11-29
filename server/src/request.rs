use bytes::{BufMut, BytesMut};
use mio::net::TcpStream;

use serde::Deserialize;
use std::collections::HashMap;
static BODY_DELIM: &[u8] = b"\r\n\r\n";

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

pub struct Request {
    pub(crate) socket: TcpStream,
    pub(crate) parse_buf: BytesMut,
    pub(crate) resp_buf: BytesMut,
}

impl Request {
    pub fn set_socket(&mut self, socket: TcpStream) {
        self.socket = socket;
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.parse_buf.clear();
        self.resp_buf.clear();
    }

    #[inline]
    fn create_html_response(mut self, status_code: &[u8], body: &[u8]) {
        self.resp_buf.put_slice(status_code);
        self.resp_buf
            .put_slice(body.len().to_string().as_bytes());
        self.resp_buf.put_slice(BODY_DELIM);
        self.resp_buf.put_slice(body);
    }
}
