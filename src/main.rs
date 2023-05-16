use std::collections::HashMap;
use std::io::{Read, Write};

// use slab::Slab;
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

fn main() {
    let address = "0.0.0.0:3000";
    let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    let mut counter: usize = 0;
    let mut sockets: HashMap<Token, TcpStream> = HashMap::new();
    let mut buffer = [0_u8; 1024];
    let buf_pool = Pool::new(300, &|| BytesMut::with_capacity(1024));

    let mut events = Events::with_capacity(1024);
    {
        // This is an incredibly dumb solution to both requests and buf_pool
        // going out of scope at the same time.
        // AKA the borrow checker getting in the way.
        let mut requests: HashMap<Token, Reusable<BytesMut>> = HashMap::new();
        loop {
            poll.poll(&mut events, None).unwrap();
            for event in &events {
                match event.token() {
                    Token(0) => loop {
                        match listener.accept() {
                            Ok((mut socket, _)) => {
                                println!("Sockets: {:?}", sockets.len());
                                counter += 1;
                                let token = Token(counter);
                                poll.registry()
                                    .register(&mut socket, token, Interest::READABLE)
                                    .unwrap();

                                sockets.insert(token, socket);
                                let mut buf = buf_pool.pull(|| {
                                    println!("Miss object pool allocation!");
                                    BytesMut::with_capacity(1024)
                                });
                                buf.clear();
                                requests.insert(token, buf);
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    },
                    token if event.is_readable() => {
                        let mut headers = [httparse::Header {
                            name: "",
                            value: &[],
                        }; 16];
                        let mut request_parser = httparse::Request::new(&mut headers);
                        loop {
                            let read = sockets.get_mut(&token).unwrap().read(&mut buffer);
                            match read {
                                Ok(0) => {
                                    sockets.remove(&token);
                                    break;
                                }
                                Ok(n) => {
                                    let req: &mut BytesMut = requests.get_mut(&token).unwrap();
                                    if req.remaining_mut() < n {
                                        req.reserve(n)
                                    }
                                    req.put_slice(&buffer[0..n])
                                }
                                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                                Err(_) => break,
                            }
                        }

                        if requests.contains_key(&token) {
                            let temp_req = requests.get(&token).unwrap();
                            if request_parser.parse(temp_req).unwrap().is_complete() {
                                poll.registry()
                                    .reregister(
                                        sockets.get_mut(&token).unwrap(),
                                        token,
                                        Interest::WRITABLE,
                                    )
                                    .unwrap();
                            }
                        }
                    }
                    token if event.is_writable() => {
                        requests.remove(&token).unwrap();
                        println!("req len: {:?}", requests.len());
                        let socket = sockets.get_mut(&token).unwrap();
                        socket.write_all(RESPONSE.as_bytes()).unwrap();

                        poll.registry()
                            .reregister(socket, token, Interest::READABLE)
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}
