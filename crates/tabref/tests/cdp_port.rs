#![allow(clippy::unwrap_used)]

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

#[test]
fn a_missing_version_route_suggests_a_direct_websocket_url_in_text() {
    let (run, _) = tabs_against("404 Not Found", "", false);
    assert!(run.stderr.contains("/json/version"), "{}", run.stderr);
}

#[test]
fn a_version_response_without_a_debugger_url_suggests_one_in_json() {
    let (run, hint) = tabs_against("200 OK", "{}", true);
    let value = run.json();
    let error = &value["error"];
    assert_eq!(error["details"]["kind"], "cdp_endpoint_invalid");
    assert!(error["message"].as_str().unwrap().contains("/json/version"));
    assert!(error["suggestion"].as_str().unwrap().contains(&hint));
}

fn tabs_against(status: &'static str, body: &'static str, json: bool) -> (common::Run, String) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut lines = BufReader::new(&mut stream).lines();
        let request = lines.next().unwrap().unwrap();
        for line in lines {
            if line.unwrap().is_empty() {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });

    let home = common::scratch("cdp-port");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .env("TABREF_HOME", &home.0)
        .env("TABREF_NO_DAEMON", "1")
        .args(["--cdp", &address.port().to_string()]);
    if json {
        command.arg("--json");
    }
    let output = command.arg("tabs").output().unwrap();
    let run = common::Run {
        ok: output.status.success(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    };
    assert!(!run.ok, "{}", run.stdout);
    let hint = format!("--cdp ws://{address}/devtools/browser/<id>");
    let printed = if json { &run.stdout } else { &run.stderr };
    assert!(printed.contains(&hint), "{printed}");
    assert_eq!(server.join().unwrap(), "GET /json/version HTTP/1.1");
    (run, hint)
}
