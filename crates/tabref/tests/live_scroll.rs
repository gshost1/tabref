//! Live runs of `scroll` against headless Chrome and lists that load more
//! as they are scrolled: a window list fed by an `IntersectionObserver`
//! sentinel and a delayed fetch, a scroll container fed by a debounced
//! scroll handler, a static page, and the end of each list. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_scroll -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{Harness, Run, harness, live};

const FETCH_DELAY: Duration = Duration::from_millis(350);

const SENTINEL: &str = r#"<!doctype html><title>Ledger</title><style>li{height:40px}</style>
<h1>Ledger</h1><ol id=rows>ROWS</ol><p id=tail>Fetching entries</p>
<script>
let next = 30, busy = false, done = false;
const rows = document.getElementById('rows'), tail = document.getElementById('tail');
function load() {
  if (busy || done) return;
  busy = true;
  fetch('/entries?name=Entry&total=70&from=' + next).then(r => r.json()).then(page => {
    for (const text of page.items) { const li = document.createElement('li'); li.textContent = text; rows.appendChild(li); }
    next += page.items.length; done = page.last; busy = false;
    tail.textContent = done ? 'No more entries' : 'Fetching entries';
  });
}
new IntersectionObserver(seen => { if (seen[0].isIntersecting) load(); }).observe(tail);
</script>"#;

const CONTAINER: &str = r#"<!doctype html><title>Inbox</title>
<style>html,body{margin:0;height:100%;overflow:hidden} #box{height:320px;overflow:auto} .card{height:48px}</style>
<div id=box><div id=cards>ROWS</div><div id=spin>More below</div></div>
<script>
let next = 20, busy = false, done = false, timer = 0;
const box = document.getElementById('box'), cards = document.getElementById('cards');
function load() {
  if (busy || done) return;
  busy = true;
  fetch('/entries?name=Message&total=50&from=' + next).then(r => r.json()).then(page => {
    for (const text of page.items) { const div = document.createElement('div'); div.className = 'card'; div.innerHTML = '<a href=#more></a>'; div.firstChild.textContent = text; cards.appendChild(div); }
    next += page.items.length; done = page.last; busy = false;
    if (done) document.getElementById('spin').textContent = 'That is everything';
  });
}
box.addEventListener('scroll', () => {
  clearTimeout(timer);
  timer = setTimeout(() => { if (box.scrollTop + box.clientHeight >= box.scrollHeight - 80) load(); }, 60);
});
</script>"#;

const STATIC: &str = r#"<!doctype html><title>Essay</title><style>p{height:60px}</style>
<h1>Essay</h1>ROWS<footer>The end of the essay</footer>"#;

fn rows(tag: &str, name: &str, count: usize) -> String {
    (1..=count)
        .map(|n| format!("<{tag} class=card><a href=\"#{n}\">{name} {n}</a></{tag}>"))
        .collect()
}

fn site() -> (Harness, String) {
    let pages = HashMap::from([
        (
            "/ledger".to_owned(),
            SENTINEL.replace("ROWS", &rows("li", "Entry", 30)),
        ),
        (
            "/inbox".to_owned(),
            CONTAINER.replace("ROWS", &rows("div", "Message", 20)),
        ),
        (
            "/essay".to_owned(),
            STATIC.replace("ROWS", &rows("p", "Paragraph", 60)),
        ),
    ]);
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
    (harness("scroll"), format!("http://127.0.0.1:{port}"))
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
        std::thread::sleep(FETCH_DELAY);
        ("application/json", entries(query))
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

fn entries(query: &str) -> String {
    let field = |key: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
            .unwrap_or_default()
            .to_owned()
    };
    let from: usize = field("from").parse().unwrap_or(0);
    let total: usize = field("total").parse().unwrap_or(0);
    let name = field("name");
    let to = (from + 20).min(total);
    let items: Vec<String> = (from + 1..=to).map(|n| format!("{name} {n}")).collect();
    serde_json::json!({ "items": items, "last": to >= total }).to_string()
}

fn batch(tabref: &Harness, steps: &str) -> Run {
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

#[test]
fn a_scroll_that_reaches_a_sentinel_waits_for_the_rows_it_fetched() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/ledger")]);
    let run = tabref.ok(&["scroll", "down"]);
    let first = run.stdout.lines().next().unwrap_or_default();
    assert!(first.starts_with("scrolled down 600px"), "{}", run.stdout);
    assert!(first.contains("20 new items"), "{}", run.stdout);
    assert!(run.stdout.contains("+ Entry 31"), "{}", run.stdout);
    let count = tabref.ok(&["js", "document.querySelectorAll('li').length"]);
    assert_eq!(count.stdout.trim(), "50");
}

#[test]
fn a_batch_of_scrolls_reaches_the_end_of_a_lazy_list() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/ledger")]);
    let run = batch(&tabref, &"scroll down\n".repeat(8));
    assert!(run.ok, "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("Entry 70"), "{}", run.stdout);
    let last = run.stdout.lines().last().unwrap_or_default();
    assert!(
        last.contains("end of list, nothing more loaded"),
        "{}",
        run.stdout
    );
    let count = tabref.ok(&["js", "document.querySelectorAll('li').length"]);
    assert_eq!(count.stdout.trim(), "70");
}

#[test]
fn a_scroll_container_with_a_debounced_loader_loads_through_its_end() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/inbox")]);
    let run = tabref.ok(&["scroll", "down", "--px", "2000"]);
    assert!(run.stdout.contains("20 new items"), "{}", run.stdout);
    assert!(run.stdout.contains("+ Message 21"), "{}", run.stdout);
    let held = tabref.run(&["scroll", "down", "Message 3", "--px", "2000"]);
    assert!(held.ok, "{}\n{}", held.stdout, held.stderr);
    assert!(held.stdout.contains("10 new items"), "{}", held.stdout);
    assert!(held.stdout.contains("+ Message 50"), "{}", held.stdout);
    let last = tabref.ok(&["scroll", "down", "--px", "2000"]);
    assert!(
        last.stdout.contains("end of list: nothing more loaded"),
        "{}",
        last.stdout
    );
    let end = tabref.ok(&["scroll", "down", "--px", "2000"]);
    let first = end.stdout.lines().next().unwrap_or_default();
    assert!(first.starts_with("did not move"), "{}", end.stdout);
    assert!(first.contains("nothing more loaded"), "{}", end.stdout);
}

#[test]
fn a_static_page_scrolls_at_once_and_says_where_it_ends() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/essay")]);
    let middle = tabref.ok(&["scroll", "down"]);
    assert_eq!(middle.stdout.trim(), "scrolled down 600px");
    let started = Instant::now();
    let end = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert!(started.elapsed() < Duration::from_millis(3_000));
    assert!(
        end.stdout.contains("end of list: nothing more loaded"),
        "{}",
        end.stdout
    );
    let stuck = tabref.ok(&["scroll", "down"]);
    assert_eq!(
        stuck.stdout.trim(),
        "did not move: end of list, nothing more loaded"
    );
    let back = tabref.ok(&["scroll", "up"]);
    assert_eq!(back.stdout.trim(), "scrolled up 600px");
}
