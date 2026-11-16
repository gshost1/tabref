//! Live runs of `scroll` against loaders that start late and pages whose
//! DOM changes without loading anything: a scroll handler debounced by
//! 400 ms before a 300 ms fetch, an `IntersectionObserver` that waits on a
//! timer, a ticking clock, loaded nodes that show no text, and loaded text
//! with control characters or one very long line. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_scroll_shapes`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::lazy_site::{lazy_site, rows};
use common::{Harness, harness, live};
use serde_json::Value;

const DEBOUNCED: &str = r#"<!doctype html><title>Inbox</title>
<style>html,body{margin:0;height:100%;overflow:hidden} #box{height:320px;overflow:auto} .card{height:48px}</style>
<div id=box><div id=cards>ROWS</div></div>
<script>
let timer = 0, busy = false;
const box = document.getElementById('box');
box.addEventListener('scroll', () => {
  clearTimeout(timer);
  timer = setTimeout(() => {
    if (busy || box.scrollTop + box.clientHeight < box.scrollHeight - 80) return;
    busy = true;
    fetch('/entries?name=Message&total=40&from=20&delay=300').then(r => r.json()).then(page => {
      for (const text of page.items) { const div = document.createElement('div'); div.className = 'card'; div.textContent = text; document.getElementById('cards').appendChild(div); }
    });
  }, 400);
});
</script>"#;

const TIMED: &str = r#"<!doctype html><title>Timed</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><p id=tail></p>
<script>
let done = false;
new IntersectionObserver(seen => {
  if (!seen[0].isIntersecting || done) return;
  done = true;
  setTimeout(() => {
    for (let i = 31; i <= 35; i++) { const li = document.createElement('li'); li.textContent = 'Entry ' + i; document.getElementById('rows').appendChild(li); }
  }, 400);
}).observe(document.getElementById('tail'));
</script>"#;

const TICKER: &str = r#"<!doctype html><title>Essay</title><style>p{height:60px}</style>
<header id=clock></header>ROWS<footer>The end of the essay</footer>
<script>
let n = 0;
setInterval(() => { n += 1; document.getElementById('clock').innerHTML = '<b>Tick ' + n + '</b>'; }, 30);
</script>"#;

const MIXED: &str = r#"<!doctype html><title>Mixed</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><p id=tail></p>
<script>
let done = false;
new IntersectionObserver(seen => {
  if (!seen[0].isIntersecting || done) return;
  done = true;
  const rows = document.getElementById('rows');
  rows.insertAdjacentHTML('beforeend', '<script>var hiddenCode = 1;<\/script><style>.secret{color:red}</style>'
    + '<template><li>Template row</li></template><noscript>No script row</noscript><li hidden>Hidden row</li>');
  const odd = document.createElement('li');
  odd.textContent = 'Esc\u001b[31mred\rback';
  rows.appendChild(odd);
  const long = document.createElement('li');
  long.textContent = 'Long ' + 'x'.repeat(50000);
  rows.appendChild(long);
  for (let i = 1; i <= 3; i++) { const li = document.createElement('li'); li.textContent = 'Short ' + i; rows.appendChild(li); }
}).observe(document.getElementById('tail'));
</script>"#;

fn site() -> (Harness, String) {
    let base = lazy_site(vec![
        (
            "/debounced",
            DEBOUNCED.replace("ROWS", &rows("div", "Message", 20)),
        ),
        ("/timed", TIMED.replace("ROWS", &rows("li", "Entry", 30))),
        (
            "/ticker",
            TICKER.replace("ROWS", &rows("p", "Paragraph", 60)),
        ),
        ("/mixed", MIXED.replace("ROWS", &rows("li", "Entry", 30))),
    ]);
    (harness("scroll-shapes"), base)
}

fn find_key<'v>(value: &'v Value, key: &str) -> Option<&'v Value> {
    match value {
        Value::Object(map) => map
            .get(key)
            .or_else(|| map.values().find_map(|inner| find_key(inner, key))),
        Value::Array(items) => items.iter().find_map(|inner| find_key(inner, key)),
        _ => None,
    }
}

#[test]
fn a_scroll_handler_debounced_by_400_ms_before_a_300_ms_fetch_is_waited_for() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/debounced")]);
    let run = tabref.ok(&["scroll", "down", "--px", "5000"]);
    assert!(run.stdout.contains("20 new items loaded"), "{}", run.stdout);
    assert!(run.stdout.contains("+ Message 21"), "{}", run.stdout);
}

#[test]
fn an_observer_that_waits_on_a_timer_is_waited_for() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/timed")]);
    let run = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert!(run.stdout.contains("5 new items loaded"), "{}", run.stdout);
}

#[test]
fn a_ticking_clock_is_not_new_items_and_costs_little() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/ticker")]);
    let started = Instant::now();
    let middle = tabref.ok(&["scroll", "down"]);
    assert_eq!(middle.stdout.trim(), "scrolled down 600px");
    assert!(started.elapsed() < Duration::from_millis(1_000));
    let started = Instant::now();
    let end = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert!(started.elapsed() < Duration::from_millis(1_500));
    assert!(!end.stdout.contains("Tick"), "{}", end.stdout);
    assert!(!end.stdout.contains("new item"), "{}", end.stdout);
}

#[test]
fn loaded_nodes_without_shown_text_are_not_items_and_lines_are_cut_and_escaped() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/mixed")]);
    let run = tabref.ok(&["--json", "scroll", "down", "--px", "20000"]);
    let json = run.json();
    assert_eq!(
        find_key(&json, "new_items"),
        Some(&Value::from(5)),
        "{json}"
    );
    let lines: Vec<&str> = find_key(&json, "new_lines")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(lines.len(), 5, "{lines:?}");
    assert!(
        lines.iter().all(|line| line.chars().count() <= 100),
        "{lines:?}"
    );
    assert!(lines[1].starts_with("Long xxx"), "{lines:?}");
    assert_eq!(lines[4], "Short 3");
    for hidden in [
        "hiddenCode",
        "secret",
        "Template row",
        "No script row",
        "Hidden row",
    ] {
        assert!(!run.stdout.contains(hidden), "{hidden}: {}", run.stdout);
    }
    tabref.ok(&["open", &format!("{base}/mixed")]);
    let printed = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert!(!printed.stdout.contains('\u{1b}'), "{:?}", printed.stdout);
    assert!(!printed.stdout.contains('\r'), "{:?}", printed.stdout);
    assert!(printed.stdout.contains("+ Short 3"), "{}", printed.stdout);
    assert!(
        printed
            .stdout
            .lines()
            .all(|line| line.chars().count() <= 110)
    );
}
