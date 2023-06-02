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
                                    .register(&mut socket, Token(key), Interest::READABLE)
                                    .unwrap();
                                next.insert(Connection{
                                    socket,
                                    buf: buf.into(),
                                });
                                println!("Setting up initial socket: {:?}", sockets.len());
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                        println!("Done setting up initial socket: {:?}", sockets.len());
                    },
                    token if event.is_readable() => {
                        println!("Reading socket: {:?}", sockets.len());
                        receive_request(token, &mut sockets, &mut poll)
                    },
                    token if event.is_writable() => {
                        println!("Sockets when writing: {:?}", sockets.len());
                        let socket = sockets.get_mut(token.0).unwrap();
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

fn receive_request(token: Token, sockets: &mut Slab<Connection>, poll: &mut Poll) {
    let mut headers = [httparse::Header {
        name: "",
        value: &[],
    }; 16];
    let mut request_parser = httparse::Request::new(&mut headers);
    let mut buffer = [0_u8; 1024];
    println!("Sockets: {:?}", sockets.len());
    loop {
        let conn = sockets.get_mut(token.0).unwrap();
        let read = conn.socket.read(&mut buffer);
        match read {
            Ok(0) => {
                sockets.remove(token.0);
                println!("Removing socket: {:?}", sockets.len());
                break;
            }
            Ok(n) => {
                let req: &mut BytesMut = &mut conn.buf;
                if req.remaining_mut() < n {
                    req.reserve(n)
                }
                req.put_slice(&buffer[0..n])
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(_) => break,
        }
    }

    if let Some(conn) = sockets.get(token.0) {
        if request_parser.parse(&conn.buf).unwrap().is_complete() {
            if let Some(socket) = sockets.get_mut(token.0) {
                poll.registry()
                    .reregister(&mut socket.socket, token, Interest::WRITABLE)
                    .unwrap();
            }
        }
    }
}
