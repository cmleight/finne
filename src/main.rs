use std::collections::HashMap;
use std::io::{Read, Write};

use mio::net::{TcpListener, TcpStream};
use mio::{Events, Interest, Poll, Token};

mod http;

static RESPONSE: &str = "HTTP/1.1 200 OK
Content-Type: text/html
Connection: keep-alive
Content-Length: 6

hello
";

fn is_double_crnl(window: &[u8]) -> bool {
    window.len() >= 4
        && (window[0] == b'\r')
        && (window[1] == b'\n')
        && (window[2] == b'\r')
        && (window[3] == b'\n')
}

fn main() {
    let address = "0.0.0.0:3000";
    let mut listener = TcpListener::bind(address.parse().unwrap()).unwrap();

    let mut poll = Poll::new().unwrap();
    poll.registry()
        .register(&mut listener, Token(0), Interest::READABLE)
        .unwrap();

    let mut counter: usize = 0;
    let mut sockets: HashMap<Token, TcpStream> = HashMap::new();
    let mut requests: HashMap<Token, Vec<u8>> = HashMap::new();
    let mut buffer = [0_u8; 1024];

    let mut events = Events::with_capacity(1024);
    loop {
        poll.poll(&mut events, None).unwrap();
        for event in &events {
            match event.token() {
                Token(0) => loop {
                    match listener.accept() {
                        Ok((mut socket, _)) => {
                            counter += 1;
                            let token = Token(counter);
                            poll.registry().register(&mut socket, token, Interest::READABLE).unwrap();

                            sockets.insert(token, socket);
                            requests.insert(token, Vec::with_capacity(192));
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(_) => break,
                    }
                },
                token if event.is_readable() => {
                    loop {
                        let read = sockets.get_mut(&token).unwrap().read(&mut buffer);
                        match read {
                            Ok(0) => {
                                sockets.remove(&token);
                                break;
                            }
                            Ok(n) => {
                                let req = requests.get_mut(&token).unwrap();
                                for b in &buffer[0..n] {
                                    req.push(*b);
                                }
                            }
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    }

                    let ready = requests.get(&token).unwrap().windows(4).any(is_double_crnl);

                    if ready {
                        poll.registry().reregister(sockets.get_mut(&token).unwrap(), token, Interest::WRITABLE).unwrap();
                    }
                }
                token if event.is_writable() => {
                    requests.get_mut(&token).unwrap().clear();
                    let socket = sockets.get_mut(&token).unwrap();
                    socket
                        .write_all(RESPONSE.as_bytes())
                        .unwrap();

                    poll.registry().reregister(socket, token, Interest::READABLE).unwrap();
                }
                _ => unreachable!(),
            }
        }
    }
}

