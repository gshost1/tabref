use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use tabref_core::{AppError, Deadline, ErrorCode};

use super::{connect, verify_connection};
use crate::ScriptedTransport;
use crate::{BrowserEndpoint, CdpConnection, EndpointSource, WebSocketUrl};

fn endpoint(source: EndpointSource) -> BrowserEndpoint {
    BrowserEndpoint {
        websocket_url: WebSocketUrl::loopback(9222, "/devtools/browser/x").unwrap(),
        source,
        product: None,
    }
}

fn verify_against_silence(source: EndpointSource) -> AppError {
    let transport = ScriptedTransport::new().expect_silence("Browser.getVersion");
    let mut connection = CdpConnection::new(transport.boxed());
    verify_connection(
        &mut connection,
        &endpoint(source),
        Deadline::after(50).unwrap(),
    )
    .unwrap_err()
}

#[test]
fn an_answering_browser_is_verified() {
    let transport =
        ScriptedTransport::new().expect("Browser.getVersion", json!({ "product": "Chrome/144" }));
    let mut connection = CdpConnection::new(transport.boxed());

    let version = verify_connection(
        &mut connection,
        &endpoint(EndpointSource::DevToolsActivePort),
        Deadline::after(500).unwrap(),
    )
    .unwrap();

    assert_eq!(version["product"], "Chrome/144");
}

#[test]
fn a_silent_active_port_browser_reports_pending_approval() {
    let error = verify_against_silence(EndpointSource::DevToolsActivePort);
    assert_eq!(error.code, ErrorCode::PermDenied);
    let details = error.details.as_ref().unwrap();
    assert_eq!(details["kind"], "cdp_approval_pending");
    assert_eq!(details["port"], 9222);
    assert!(error.suggestion.unwrap().contains("Allow remote debugging"));
}

fn listening_endpoint(source: EndpointSource) -> (TcpListener, BrowserEndpoint) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = BrowserEndpoint {
        websocket_url: WebSocketUrl::loopback(port, "/devtools/browser/x").unwrap(),
        source,
        product: None,
    };
    (listener, endpoint)
}

#[test]
fn an_upgrade_that_stalls_is_retried_on_a_fresh_connection() {
    let (listener, endpoint) = listening_endpoint(EndpointSource::JsonVersion);
    let server = thread::spawn(move || {
        let (silent, _) = listener.accept().unwrap();
        let (answering, _) = listener.accept().unwrap();
        let mut socket = tungstenite::accept(answering).unwrap();
        drop(silent);
        while socket.read().is_ok() {}
    });

    let started = Instant::now();
    let connection = connect(&endpoint, Deadline::after(4_000).unwrap());

    assert!(connection.is_ok());
    assert!(started.elapsed() < Duration::from_millis(3_500));
    drop(connection);
    server.join().unwrap();
}

#[test]
fn an_upgrade_held_for_approval_is_never_asked_twice() {
    let (listener, endpoint) = listening_endpoint(EndpointSource::DevToolsActivePort);
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let mut accepted = Vec::new();
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(2_800) {
            if let Ok((stream, _)) = listener.accept() {
                accepted.push(stream);
            }
            thread::sleep(Duration::from_millis(5));
        }
        accepted.len()
    });

    let Err(error) = connect(&endpoint, Deadline::after(2_200).unwrap()) else {
        panic!("a held upgrade connected");
    };

    assert_eq!(error.details.unwrap()["kind"], "cdp_approval_pending");
    assert_eq!(server.join().unwrap(), 1);
}

#[test]
fn a_silent_port_endpoint_stays_a_timeout() {
    let error = verify_against_silence(EndpointSource::JsonVersion);
    assert_eq!(error.code, ErrorCode::Timeout);
}
