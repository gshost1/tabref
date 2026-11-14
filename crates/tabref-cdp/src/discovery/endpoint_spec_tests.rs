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
        EndpointSpec::Port(9333)
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
