//! Live runs of how long an action waits for the requests it set off,
//! against headless Chrome and a local server whose replies are marked
//! `Cache-Control: no-store`, as many apps mark their API replies. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_settle_wait`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use common::{harness, live};

const PAGE: &str = r#"<!doctype html><title>Alerts</title><h1>Alert rules</h1>
<button id=save>Keep rules</button><p id=note>Not kept yet</p>
<script>
document.getElementById('save').addEventListener('click', () => {
  fetch('/rules', { method: 'POST', body: 'on' }).then(reply => {
    document.getElementById('note').textContent = reply.ok ? 'Rules kept' : 'Failed';
  });
});
</script>"#;

const SLOWEST_SETTLE: Duration = Duration::from_millis(1_000);

fn serve_no_store() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || answer(stream));
        }
    });
    port
}

fn answer(mut stream: TcpStream) {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => request.extend_from_slice(&chunk[..read]),
        }
    }
    let head = String::from_utf8_lossy(&request).into_owned();
    let (kind, body) = if head.starts_with("POST") {
        let length = head
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
            })
            .unwrap_or(0);
        let start = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let mut have = request.len() - start;
        while have < length {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => have += read,
            }
        }
        ("application/json", r#"{"kept":true}"#.to_owned())
    } else {
        ("text/html; charset=utf-8", PAGE.to_owned())
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}

#[test]
fn a_click_whose_unread_no_store_reply_has_fully_arrived_does_not_wait_out_the_network_cap() {
    if !live() {
        return;
    }
    let port = serve_no_store();
    let tabref = harness("settle-no-store");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    tabref.ok(&["click", "Keep rules"]);
    let started = Instant::now();
    let run = tabref.ok(&["click", "Keep rules"]);
    let took = started.elapsed();
    assert!(
        took < SLOWEST_SETTLE,
        "the click took {took:?}, so it waited out a cap:\n{}",
        run.stdout
    );
}
