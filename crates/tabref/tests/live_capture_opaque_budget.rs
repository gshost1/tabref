#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;

#[test]
fn four_large_opaque_sheets_get_a_classification_budget() {
    if !common::live() {
        return;
    }
    let css = format!(
        ".unused{{color:red;padding:1px;--fixture:\"{}\"}}",
        "x".repeat(100)
    )
    .repeat(2_000);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let css_port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 8192];
            let _ = stream.read(&mut request);
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{css}",
                css.len()
            );
        }
    });
    let links: String = ['a', 'b', 'c', 'd']
        .iter()
        .map(|name| format!("<link rel=stylesheet href='http://127.0.0.1:{css_port}/{name}.css'>"))
        .collect();
    let html = format!("<!doctype html><head>{links}</head><body><button>Go</button></body>");
    let port = common::serve(&[("/", &html)]);
    let harness = common::harness("opaque-budget");
    let run = |args: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tabref"))
            .args([
                "--cdp",
                &harness.browser.port.to_string(),
                "--json",
                "--timeout",
                "30000",
            ])
            .args(args)
            .env("TABREF_HOME", &harness.home)
            .env_remove("TABREF_NO_DAEMON")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let open = run(&["open", &format!("http://127.0.0.1:{port}/")]);
    let find = run(&["find", "Go"]);
    run(&["daemon", "stop"]);
    assert_eq!(open["data"]["snapshot_id"], find["data"]["snapshot_id"]);
}
