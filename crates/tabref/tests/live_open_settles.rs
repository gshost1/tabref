//! Live runs of `open` waiting, after `load`, for the page's own load-time
//! work: a block a load handler fetches and inserts is in open's view and
//! the `find` after it reuses open's capture; a page that renders its body
//! after a fetch prints that body from `open`; a ticker or a poll costs no
//! more than the bound; a page still rendering past the longer bound says
//! so; a static page, empty or not, waits for nothing.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_open_settles`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{Harness, Run, live};

const LATE: &str = r#"<!doctype html><html><head><title>Late</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><div id="slot"></div>
<script>addEventListener('load', () => fetch('/data?ms=150').then(r => r.text()).then(t => {
  const p = document.createElement('p'); p.textContent = 'Late needle ' + t; document.getElementById('slot').append(p);
}));</script></body></html>"#;

const SPA: &str = r#"<!doctype html><html><head><title>App</title></head><body><div id="root"></div>
<script>fetch('/data?ms=200').then(r => r.text()).then(t => {
  document.getElementById('root').innerHTML = '<h1>Dashboard</h1><p>Rendered report ' + t + '</p><button>Refresh</button>';
});</script></body></html>"#;

const SLOW_SPA: &str = r#"<!doctype html><html><head><title>Slow app</title></head><body><div id="root">Loading</div>
<script>fetch('/data?ms=4000').then(r => r.text()).then(t => {
  document.getElementById('root').innerHTML = '<h1>Dashboard</h1><p>Rendered report ' + t + '</p>';
});</script></body></html>"#;

const TICKER: &str = r#"<!doctype html><html><head><title>Ticker</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><p id="clock">0</p>
<script>let n = 0; setInterval(() => { document.getElementById('clock').textContent = 'tick ' + (++n); }, 20);</script>
</body></html>"#;

const POLL: &str = r#"<!doctype html><html><head><title>Poll</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p>
<script>const go = () => fetch('/data?ms=60').then(() => setTimeout(go, 10)); go();</script>
</body></html>"#;

const STATIC: &str = r#"<!doctype html><html><head><title>Still</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p></body></html>"#;

const STUB: &str = r#"<!doctype html><html><head><title>404</title></head><body><p>Not found</p></body></html>"#;

/// Serves each `(path, html)` on a loopback port, plus `/data?ms=N`, which
/// answers a short text after N milliseconds.
fn serve(pages: &[(&str, &str)]) -> u16 {
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

fn run(tabref: &Harness, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--cdp", &tabref.browser.port.to_string(), "--json"])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON")
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

fn snapshot_id(run: &Run) -> String {
    run.json()["data"]["snapshot_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no snapshot id in {}", run.stdout))
        .to_owned()
}

/// Opens `html` and returns the run and how long it took.
fn open(tabref: &Harness, html: &'static str) -> (Run, Duration) {
    let port = serve(&[("/", html)]);
    let url = format!("http://127.0.0.1:{port}/");
    let started = Instant::now();
    let opened = run(tabref, &["open", &url]);
    (opened, started.elapsed())
}

fn stop(tabref: &Harness) {
    let _ = run(tabref, &["daemon", "stop"]);
}

#[test]
fn a_block_inserted_after_load_is_in_open_and_find_reuses_it() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-late");
    let (opened, _) = open(&tabref, LATE);
    assert!(
        opened.stdout.contains("Late needle fetched words"),
        "open missed the late block: {}",
        opened.stdout
    );
    std::thread::sleep(Duration::from_millis(900));
    let found = run(&tabref, &["find", "Late needle"]);
    assert_eq!(snapshot_id(&found), snapshot_id(&opened), "find recaptured");
    stop(&tabref);
}

#[test]
fn a_page_that_renders_after_load_prints_its_content_from_open() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-spa");
    let (opened, _) = open(&tabref, SPA);
    assert!(
        opened.stdout.contains("Rendered report fetched words"),
        "{}",
        opened.stdout
    );
    assert!(!opened.stdout.contains("still rendering"), "{}", opened.stdout);
    stop(&tabref);
}

#[test]
fn a_page_still_rendering_past_the_bound_says_so() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-slow");
    let (opened, took) = open(&tabref, SLOW_SPA);
    assert!(
        opened.stdout.contains("still rendering") && opened.stdout.contains("tabref wait"),
        "{}",
        opened.stdout
    );
    assert!(took < Duration::from_millis(3_000), "open took {took:?}");
    stop(&tabref);
}

#[test]
fn a_ticker_or_a_poll_costs_no_more_than_the_bound() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-busy");
    let (_, base) = open(&tabref, STATIC);
    for html in [TICKER, POLL] {
        let (opened, took) = open(&tabref, html);
        assert!(opened.stdout.contains("Alpha needle"), "{}", opened.stdout);
        assert!(!opened.stdout.contains("still rendering"), "{}", opened.stdout);
        assert!(
            took < base + Duration::from_millis(900),
            "open took {took:?}, a static page {base:?}"
        );
    }
    stop(&tabref);
}

#[test]
fn a_static_page_empty_or_not_waits_for_nothing() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-static");
    for html in [STATIC, STUB, "<!doctype html><title>blank</title>"] {
        let (opened, took) = open(&tabref, html);
        assert!(!opened.stdout.contains("still rendering"), "{}", opened.stdout);
        assert!(took < Duration::from_millis(1_200), "open took {took:?}");
    }
    stop(&tabref);
}
