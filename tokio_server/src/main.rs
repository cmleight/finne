use std::collections::HashMap;
use std::io::{Read, Write};
use std::ops::DerefMut;
use std::time::Duration;

use bytes::{BufMut, BytesMut};
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};
use serde::Deserialize;
use serde_json;
use slab::Slab;

use finne_parser::request_parser::HttpRequest;
use finne_parser::request_parser::Method;

const BUF_EXPANSION: usize = 1024;

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
fn pull_or_create(pool: &Pool<RequestBuffers>) -> Reusable<RequestBuffers> {
    return pool.pull(|| {
        println!("Miss object pool allocation!");
        return RequestBuffers::default();
    });
}

async fn process_request() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

fn build_connection() -> (Poll, Events, [u8; 1024]){
    let address = "0.0.0.0:3000";
    let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();

    let poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();
    let events = Events::with_capacity(1024);
    let buffer = [0_u8; 1024];

    return (poll, events, buffer);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (poll, events, buffer) = build_connection();
    let mut sockets: Slab<ConnectionData> = Slab::new();

    Ok(())
}

