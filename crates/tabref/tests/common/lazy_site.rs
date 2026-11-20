//! A loopback site for scroll tests: fixed pages, plus `/entries`, which
//! answers `{"items": [...], "last": bool}` for `name`, `total`, `from`
//! and an optional `count` (20) after an optional `delay` in milliseconds
//! (350).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use super::{Harness, Run};

const DEFAULT_DELAY_MS: u64 = 350;

/// Serves `pages` on a loopback port and returns the site's base URL.
pub fn lazy_site(pages: Vec<(&str, String)>) -> String {
    let pages: HashMap<String, String> = pages
        .into_iter()
        .map(|(path, html)| (path.to_owned(), html))
        .collect();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let pages = Arc::new(pages);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let pages = Arc::clone(&pages);
            std::thread::spawn(move || answer(stream, &pages));
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// `count` rows `<tag class=card><a href="#n">name n</a></tag>`.
pub fn rows(tag: &str, name: &str, count: usize) -> String {
    (1..=count)
        .map(|n| format!("<{tag} class=card><a href=\"#{n}\">{name} {n}</a></{tag}>"))
        .collect()
}

/// Runs `tabref do` with `steps` on its standard input.
pub fn batch(tabref: &Harness, steps: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--cdp", &tabref.browser.port.to_string(), "do"])
        .env("TABREF_HOME", &tabref.home)
        .env("TABREF_REFS", "long")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(steps.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
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
    let (kind, body) = if path == "/entries" {
        let delay = field(query, "delay").parse().unwrap_or(DEFAULT_DELAY_MS);
        std::thread::sleep(Duration::from_millis(delay));
        ("application/json", entries(query))
    } else {
        let page = pages.get(path).cloned().unwrap_or_default();
        ("text/html; charset=utf-8", page)
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

fn field(query: &str, key: &str) -> String {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .unwrap_or_default()
        .to_owned()
}

fn entries(query: &str) -> String {
    let from: usize = field(query, "from").parse().unwrap_or(0);
    let total: usize = field(query, "total").parse().unwrap_or(0);
    let count: usize = field(query, "count").parse().unwrap_or(20);
    let name = field(query, "name");
    let to = (from + count).min(total);
    let items: Vec<String> = (from + 1..=to).map(|n| format!("{name} {n}")).collect();
    serde_json::json!({ "items": items, "last": to >= total }).to_string()
}
