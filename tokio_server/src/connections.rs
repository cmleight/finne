use std::boxed::Box;
use std::io::ErrorKind::WouldBlock;
use std::io::{Read, Write};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use mio::{Events, Poll, Interest, Token};
use mio::net::{TcpListener, TcpStream};
use bytes::BytesMut;
use slab::Slab;

pub struct ConnectionSet {
    connections: Mutex<HashMap<Arc<String>, ConnectionConfig>>,
    requests: Slab<Request>,
}

impl ConnectionSet {
    pub fn new() -> ConnectionSet {
        ConnectionSet {
            connections: Mutex::new(HashMap::new()),
            requests: Slab::with_capacity(1024),
        }
    }

    pub fn add_connection(&mut self, id: Arc<String>, address: String) {
        let mut connections = self.connections.lock().unwrap();
        connections.insert(id, ConnectionConfig::new(address));
    }

    pub fn remove_connection(&mut self, id: String) {
        let mut connections = self.connections.lock().unwrap();
        connections.remove(&id);
    }
}

pub struct ConnectionConfig {
    poll: Poll,
    address: String,
    listener: TcpListener,
    events: Events,
}

impl ConnectionConfig {
    pub fn new(address: String) -> ConnectionConfig {
        let poll = Poll::new().unwrap();
        let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();
        poll.registry().register(&mut listener, Token(0), Interest::READABLE).unwrap();
        return ConnectionConfig {
            poll,
            address,
            listener,
            events: Events::with_capacity(1024),
        }
    }

    pub fn get_request(&mut self) {
        self.poll.poll(&mut self.events, Some(Duration::from_secs(0))).unwrap();
    }
}

#[derive(Debug)]
pub struct Request {
    token: Token,
    socket: TcpStream,
    is_open: bool,
    recv_stream: BytesMut,
    send_stream: BytesMut,
    buffer: Box<[u8; 1024]>,
}

impl Request {
    fn init(token: Token, socket: TcpStream) -> Request {
        Request {
            token,
            socket,
            is_open: true,
            recv_stream: BytesMut::new(),
            send_stream: BytesMut::new(),
            buffer: Box::new([0_u8; 1024]),
        }
    }

    fn recieve(&mut self) {
        loop {
            let read = self.socket.read(&mut *self.buffer);
            match read {
                Ok(0) => {
                    self.is_open = false;
                    return
                }
                Ok(n) => {
                    self.recv_stream.extend_from_slice(&self.buffer[..n]);
                }
                Err(e) if e.kind() == WouldBlock => {
                    break
                }
                Err(_) => {
                    break
                }
            }
        }
    }

    fn send(&mut self) {
        loop {
            match self.socket.write_all(&self.send_stream) {
                Ok(_) => (),
                Err(_) => {
                    self.is_open = false;
                    break
                }
            }
        }
        self.send_stream.clear();
    }

    fn store(&mut self, data: &[u8]) {
        self.send_stream.extend_from_slice(data);
    }
}
