//! A loopback server whose `/data?ms=N` answers after N milliseconds, and
//! a `tabref` runner that keeps the daemon on, for the live tests of what
//! `open` waits for after `load`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use super::{Harness, Run};

/// Serves each `(path, html)` on a loopback port, plus `/data?ms=N`, which
/// answers a short text after N milliseconds.
pub fn serve(pages: &[(&str, &str)]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let pages: Arc<HashMap<String, String>> = Arc::new(
        pages
            .iter()
            .map(|(path, html)| ((*path).to_owned(), (*html).to_owned()))
            .collect(),
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let pages = Arc::clone(&pages);
            std::thread::spawn(move || answer(stream, &pages));
        }
    });
    port
}

fn answer(mut stream: TcpStream, pages: &HashMap<String, String>) {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => request.extend_from_slice(&chunk[..read]),
        }
    }
    let head = String::from_utf8_lossy(&request);
    let target = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let (kind, body) = if path == "/data" {
        let ms = query
            .strip_prefix("ms=")
            .and_then(|ms| ms.parse().ok())
            .unwrap_or(0);
        std::thread::sleep(Duration::from_millis(ms));
        ("text/plain", "fetched words".to_owned())
    } else {
        let page = pages.get(path).cloned().unwrap_or_default();
        ("text/html; charset=utf-8", page)
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

pub fn run(tabref: &Harness, args: &[&str]) -> Run {
    run_as(tabref, &["--json"], args)
}

pub fn run_as(tabref: &Harness, mode: &[&str], args: &[&str]) -> Run {
    run_env(tabref, mode, args, &[])
}

/// [`run_as`] with extra environment variables.
pub fn run_env(tabref: &Harness, mode: &[&str], args: &[&str], env: &[(&str, &str)]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--cdp", &tabref.browser.port.to_string()])
        .args(mode)
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let run = Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    assert!(
        run.ok,
        "tabref {args:?} failed:\n{}{}",
        run.stdout, run.stderr
    );
    run
}
