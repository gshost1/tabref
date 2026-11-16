use std::path::PathBuf;

use tabref_cdp::{BrowserEndpoint, EndpointSource, LaunchedBrowser, WebSocketUrl};

use super::super::launch_check::LaunchCheck;
use super::names_endpoint;

const RUN: &str = "/devtools/browser/5d1c0e3a-aaaa-4bbb-8ccc-000000000001";
const OTHER_RUN: &str = "/devtools/browser/5d1c0e3a-aaaa-4bbb-8ccc-000000000002";

fn record() -> LaunchedBrowser {
    LaunchedBrowser {
        pid: 4242,
        port: 51234,
        websocket_path: RUN.to_owned(),
        profile_dir: PathBuf::from("/home/u/.cache/tabref-profile-a"),
        headless: true,
        started: Some("x".to_owned()),
        persistent: false,
    }
}

fn answered(port: u16, path: &str) -> BrowserEndpoint {
    BrowserEndpoint {
        websocket_url: WebSocketUrl::loopback(port, path).unwrap(),
        source: EndpointSource::JsonVersion,
        product: None,
        user_agent: None,
    }
}

#[test]
fn the_recorded_run_on_the_recorded_port_is_tabrefs() {
    assert!(names_endpoint(&record(), &answered(51234, RUN), |_| {
        LaunchCheck::Alive
    }));
}

#[test]
fn another_browser_on_the_recorded_port_is_not_tabrefs_even_while_the_pid_lives() {
    assert!(!names_endpoint(
        &record(),
        &answered(51234, OTHER_RUN),
        |_| { LaunchCheck::Alive }
    ));
}

#[test]
fn a_record_its_check_refuses_or_another_port_is_not_tabrefs() {
    assert!(!names_endpoint(&record(), &answered(51234, RUN), |_| {
        LaunchCheck::Gone
    }));
    assert!(!names_endpoint(&record(), &answered(51235, RUN), |_| {
        LaunchCheck::Alive
    }));
}
