#![allow(clippy::unwrap_used)]

mod common;

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn a_port_that_closes_without_http_suggests_a_direct_websocket_url_in_text() {
    let (run, _) = tabs_against("", false);
    assert!(run.stderr.contains("/json/version"), "{}", run.stderr);
}

#[test]
fn a_version_response_without_a_debugger_url_suggests_one_in_json() {
    let (run, hint) = tabs_against("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}", true);
    let value = run.json();
    let error = &value["error"];
    assert_eq!(error["details"]["kind"], "cdp_endpoint_invalid");
    assert!(error["message"].as_str().unwrap().contains("/json/version"));
    assert!(error["suggestion"].as_str().unwrap().contains(&hint));
}

fn tabs_against(reply: &'static str, json: bool) -> (common::Run, String) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(error),
            }
        };
        stream.set_nonblocking(false).unwrap();
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
        stream.write_all(reply.as_bytes()).unwrap();
        Ok(request)
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
    assert_eq!(
        server.join().unwrap().unwrap(),
        "GET /json/version HTTP/1.1"
    );
    (run, hint)
}
