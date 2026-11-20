use tabref_core::{Deadline, ErrorCode};

use super::parse_version_body;
use crate::EndpointSource;

#[test]
fn an_expired_deadline_before_connect_does_not_suggest_a_websocket_url() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let error = super::endpoint_from_address(address, Deadline::after(0).unwrap()).unwrap_err();
    assert_eq!(error.kind(), Some("cdp_timeout"));
    assert!(
        !error
            .suggestion
            .as_deref()
            .unwrap_or_default()
            .contains("ws://"),
        "{error:?}",
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock,
    );
}

#[test]
fn a_loopback_debugger_url_on_the_same_port_is_accepted() {
    let body = r#"{"Browser":"Chrome/144.0.0.0","webSocketDebuggerUrl":"ws://127.0.0.1:9222/devtools/browser/abc"}"#;
    let endpoint = parse_version_body(9222, body).unwrap();
    assert_eq!(endpoint.port(), 9222);
    assert_eq!(endpoint.source, EndpointSource::JsonVersion);
    assert_eq!(endpoint.product.as_deref(), Some("Chrome/144.0.0.0"));
    assert_eq!(endpoint.websocket_url.path(), "/devtools/browser/abc");
}

#[test]
fn a_remote_or_mismatched_debugger_url_is_refused() {
    let remote = r#"{"webSocketDebuggerUrl":"ws://192.168.1.4:9222/devtools/browser/abc"}"#;
    assert_eq!(
        parse_version_body(9222, remote).unwrap_err().code,
        ErrorCode::ActionFailed
    );
    let other_port = r#"{"webSocketDebuggerUrl":"ws://127.0.0.1:9333/devtools/browser/abc"}"#;
    assert_eq!(
        parse_version_body(9222, other_port).unwrap_err().code,
        ErrorCode::ActionFailed
    );
    assert!(parse_version_body(9222, "{}").is_err());
    assert!(parse_version_body(9222, "<html>").is_err());
}

#[test]
fn a_port_that_answers_404_says_it_speaks_websocket_only_and_what_to_run() {
    use std::io::{Read, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        stream
            .write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length:0\r\nContent-Type:text/html\r\n\r\n",
            )
            .unwrap();
    });
    let deadline = tabref_core::Deadline::after(2_000).unwrap();
    let error = super::endpoint_from_port(port, deadline).unwrap_err();
    server.join().unwrap();
    assert_eq!(error.kind(), Some("cdp_no_http_route"));
    assert!(error.message.contains("HTTP 404"), "{}", error.message);
    let hint = error.suggestion.unwrap_or_default();
    assert!(hint.contains("tabref connect --user-chrome"), "{hint}");
    assert!(hint.contains(&format!("ws://127.0.0.1:{port}")), "{hint}");
}
