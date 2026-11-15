use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::json;
use tabref_core::{AppError, Deadline, ErrorCode};

use super::{connect_held, verify_connection};
use crate::ScriptedTransport;
use crate::discovery::held_port::HeldPort;
use crate::{BrowserEndpoint, CdpConnection, EndpointSource, WebSocketUrl};

#[test]
fn a_checked_port_is_reached_only_at_127_0_0_1_on_that_port() {
    let held = HeldPort {
        port: 9222,
        inodes: BTreeSet::from([1]),
        proc_root: PathBuf::from("/nonexistent"),
    };
    for url in [
        "ws://127.0.0.1:9333/devtools/browser/x",
        "ws://[::1]:9222/devtools/browser/x",
    ] {
        let endpoint = BrowserEndpoint {
            websocket_url: WebSocketUrl::parse(url).unwrap(),
            source: EndpointSource::JsonVersion,
            product: None,
            user_agent: None,
        };
        let Err(error) = connect_held(&endpoint, &held, Deadline::after(500).unwrap()) else {
            panic!("{url} was connected to");
        };
        assert_eq!(error.kind(), Some("cdp_endpoint_mismatch"), "{url}");
    }
}

fn endpoint(source: EndpointSource) -> BrowserEndpoint {
    BrowserEndpoint {
        websocket_url: WebSocketUrl::loopback(9222, "/devtools/browser/x").unwrap(),
        source,
        product: None,
        user_agent: None,
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

#[test]
fn a_silent_port_endpoint_stays_a_timeout() {
    let error = verify_against_silence(EndpointSource::JsonVersion);
    assert_eq!(error.code, ErrorCode::Timeout);
}
