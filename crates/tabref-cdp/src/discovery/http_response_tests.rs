use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use super::{http_ok_body, read_http_response, response_complete};

#[test]
fn a_response_is_complete_once_content_length_bytes_arrive() {
    let partial = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n{\"a\":";
    let full = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n{\"a\":\"bc\"}";
    assert!(!response_complete(partial));
    assert!(response_complete(full));
    assert!(!response_complete(b"HTTP/1.1 200 OK\r\nContent-Le"));
}

#[test]
fn only_a_200_response_yields_a_body() {
    assert_eq!(
        http_ok_body(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}").as_deref(),
        Some("{}")
    );
    assert!(http_ok_body(b"HTTP/1.1 404 Not Found\r\n\r\nnope").is_none());
    assert!(http_ok_body(b"garbage").is_none());
}

#[test]
fn a_server_that_holds_the_connection_open_is_read_without_waiting_for_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
            .unwrap();
        std::thread::sleep(Duration::from_millis(800));
    });
    let mut stream = TcpStream::connect(address).unwrap();
    let started = Instant::now();
    let raw = read_http_response(&mut stream, Duration::from_secs(5)).unwrap();
    assert!(started.elapsed() < Duration::from_millis(700));
    assert_eq!(http_ok_body(&raw).as_deref(), Some("{}"));
    server.join().unwrap();
}

#[test]
fn the_status_is_read_from_an_http_status_line_only() {
    use super::http_status;
    assert_eq!(
        http_status(b"HTTP/1.1 404 Not Found\r\nContent-Length:0\r\n\r\n"),
        Some(404)
    );
    assert_eq!(http_status(b"HTTP/1.1 200 OK\r\n\r\n{}"), Some(200));
    assert_eq!(http_status(b"garbage 404"), None);
    assert_eq!(http_status(b""), None);
}
