use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread;
use std::time::{Duration, Instant};

use tabref_core::Deadline;

use super::http_get_loopback;

#[test]
fn a_server_that_takes_the_connection_and_stays_silent_is_asked_again() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address: SocketAddr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (silent, _) = listener.accept().unwrap();
        let (mut answering, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = answering.read(&mut request).unwrap();
        answering
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
            .unwrap();
        drop(silent);
    });

    let started = Instant::now();
    let body = http_get_loopback(address, "/json/version", Deadline::after(4_000).unwrap());

    assert_eq!(body.unwrap(), "{}");
    assert!(started.elapsed() < Duration::from_millis(3_500));
    server.join().unwrap();
}
