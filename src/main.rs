use std::io::{Read, Write};

use slab::Slab;
use bytes::{BufMut, BytesMut};
use httparse;
use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};
use object_pool::{Pool, Reusable};

static RESPONSE: &str = "HTTP/1.1 200 OK
Content-Type: text/html
Connection: keep-alive
Content-Length: 6

hello
";

struct Connection<'a> {
    socket: TcpStream,
    buf: Reusable<'a, BytesMut>,
}

fn main() {
    let address = "0.0.0.0:3000";
    let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    let buf_pool = Pool::new(300, &|| BytesMut::with_capacity(1024));

    let mut events = Events::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    {
        let mut sockets: Slab<Connection> = Slab::new();
        loop {
            poll.poll(&mut events, None).unwrap();
            for event in &events {
                match event.token() {
                    Token(0) => loop {
                        match listener.accept() {
                            Ok((mut socket, _)) => {
                                let next = sockets.vacant_entry();
                                let mut buf = buf_pool.pull(|| {
                                    println!("Miss object pool allocation!");
                                    return BytesMut::with_capacity(1024);
                                });
                                buf.clear();
                                let key = next.key();
                                poll.registry()
                                    .register(&mut socket, Token(key + 1), Interest::READABLE)
                                    .unwrap();
                                next.insert(Connection{
                                    socket,
                                    buf: buf.into(),
                                });
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    },
                    token if event.is_readable() => {
                        receive_request(token.0 - 1, &mut sockets, &mut poll, &mut buffer)
                    },
                    token if event.is_writable() => {
                        let socket = sockets.get_mut(token.0 - 1).unwrap();
                        socket.socket.write_all(RESPONSE.as_bytes()).unwrap();

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

fn receive_request(token: usize, sockets: &mut Slab<Connection>, poll: &mut Poll, buffer: &mut [u8]) {
    let mut headers = [httparse::Header {
        name: "",
        value: &[],
    }; 16];
    let mut request_parser = httparse::Request::new(&mut headers);
    let conn = sockets.get_mut(token).unwrap();
    loop {
        let read = conn.socket.read(buffer);
        match read {
            Ok(0) => {
                sockets.remove(token);
                break;
            }
            Ok(n) => {
                conn.buf.put(&buffer[0..n])
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }

    if let Some(conn) = sockets.get(token) {
        if request_parser.parse(&conn.buf).unwrap().is_complete() {
            if let Some(socket) = sockets.get_mut(token) {
                poll.registry()
                    .reregister(&mut socket.socket, Token(token + 1), Interest::WRITABLE)
                    .unwrap();
            }
        }
    }
}
