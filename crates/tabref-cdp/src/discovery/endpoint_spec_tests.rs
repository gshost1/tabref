use tabref_core::{Deadline, ErrorCode};

use super::EndpointSpec;
use crate::{EndpointSource, WebSocketUrl};

#[test]
fn a_port_and_an_http_address_name_the_same_endpoint() {
    assert_eq!(
        EndpointSpec::parse("9222").unwrap(),
        EndpointSpec::Port(9222)
    );
    assert_eq!(
        EndpointSpec::parse("http://127.0.0.1:9222").unwrap(),
        EndpointSpec::Port(9222)
    );
    assert_eq!(
        EndpointSpec::parse("http://localhost:9333/json/version").unwrap(),
        EndpointSpec::Localhost(9333)
    );
    assert_eq!(
        EndpointSpec::parse("http://[::1]:9222").unwrap(),
        EndpointSpec::Ipv6Port(9222)
    );
}

#[test]
fn a_websocket_url_resolves_without_contacting_the_browser() {
    let spec = EndpointSpec::parse("ws://127.0.0.1:9222/devtools/browser/abc").unwrap();
    let endpoint = spec.resolve(Deadline::after(10).unwrap()).unwrap();
    assert_eq!(endpoint.source, EndpointSource::WebSocketUrl);
    assert_eq!(
        endpoint.websocket_url,
        WebSocketUrl::parse("ws://127.0.0.1:9222/devtools/browser/abc").unwrap()
    );
}

#[test]
fn remote_hosts_and_garbage_are_invalid_args() {
    for text in [
        "0",
        "http://10.0.0.2:9222",
        "ws://example.com:9222/devtools/browser/x",
        "chrome",
        "http://127.0.0.1",
    ] {
        let error = EndpointSpec::parse(text).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgs, "{text}");
    }
}

#[test]
fn a_devtools_server_on_ipv6_loopback_is_reached_there() {
    use std::io::{Read, Write};
    let Ok(listener) = std::net::TcpListener::bind("[::1]:0") else {
        return;
    };
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let read = stream.read(&mut request).unwrap();
        let body = format!(
            r#"{{"Browser":"Chrome/1","webSocketDebuggerUrl":"ws://[::1]:{port}/devtools/browser/x"}}"#
        );
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
        String::from_utf8_lossy(&request[..read]).into_owned()
    });
    let spec = EndpointSpec::parse(&format!("http://[::1]:{port}")).unwrap();
    let endpoint = spec.resolve(Deadline::after(2_000).unwrap()).unwrap();
    let request = server.join().unwrap();
    assert!(
        request.contains(&format!("Host: [::1]:{port}")),
        "{request}"
    );
    assert!(endpoint.websocket_url.socket_addr().is_ipv6());
}
