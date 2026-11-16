//! Live runs of `scroll` where tabref cannot know the list has ended: a
//! loader still working after the waits, a slow request, a request started
//! before the command, a tab that draws no frames, and a page that loads
//! more only through a button. None may say it is at the bottom with
//! nothing new loaded unless that is all it knows. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_scroll_doubt`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::lazy_site::{lazy_site, rows};
use common::{Harness, Run, harness, live};

const SPINNER: &str = r#"<!doctype html><title>Slow</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><p id=tail>Loading</p>
<script>
let started = false, n = 0;
const tail = document.getElementById('tail');
new IntersectionObserver(seen => {
  if (!seen[0].isIntersecting || started) return;
  started = true;
  const tick = setInterval(() => { n += 1; tail.textContent = 'Loading ' + n; }, 100);
  setTimeout(() => {
    clearInterval(tick);
    for (let i = 1; i <= 5; i++) { const li = document.createElement('li'); li.textContent = 'Late ' + i; document.getElementById('rows').appendChild(li); }
    tail.textContent = 'Done';
  }, 4000);
}).observe(tail);
</script>"#;

const SLOW_FETCH: &str = r#"<!doctype html><title>Slow fetch</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><p id=tail>More</p>
<script>
let busy = false;
new IntersectionObserver(seen => {
  if (!seen[0].isIntersecting || busy) return;
  busy = true;
  fetch('/entries?name=Late&total=40&from=30&delay=3000').then(r => r.json()).then(page => {
    for (const text of page.items) { const li = document.createElement('li'); li.textContent = text; document.getElementById('rows').appendChild(li); }
  });
}).observe(document.getElementById('tail'));
</script>"#;

const IN_FLIGHT: &str = r#"<!doctype html><title>Pending</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><p id=wait></p>
<script>
function start() {
  let n = 0;
  const wait = document.getElementById('wait');
  const tick = setInterval(() => { n += 1; wait.textContent = 'Fetching ' + n; }, 100);
  fetch('/entries?name=Late&total=40&from=30&delay=2500').then(r => r.json()).then(page => {
    clearInterval(tick);
    for (const text of page.items) { const li = document.createElement('li'); li.textContent = text; document.getElementById('rows').appendChild(li); }
  });
}
</script>"#;

const LOAD_MORE: &str = r#"<!doctype html><title>Catalogue</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><button onclick="this.textContent='Loading'">Load more</button>"#;

fn site() -> (Harness, String) {
    let base = lazy_site(vec![
        ("/spinner", SPINNER.replace("ROWS", &rows("li", "Entry", 30))),
        ("/slow", SLOW_FETCH.replace("ROWS", &rows("li", "Entry", 30))),
        ("/pending", IN_FLIGHT.replace("ROWS", &rows("li", "Entry", 30))),
        ("/more", LOAD_MORE.replace("ROWS", &rows("li", "Item", 30))),
    ]);
    (harness("scroll-doubt"), base)
}

fn first_line(run: &Run) -> &str {
    run.stdout.lines().next().unwrap_or_default()
}

fn assert_unsure(run: &Run) {
    let first = first_line(run);
    assert!(first.starts_with("scrolled down"), "{}", run.stdout);
    assert!(first.contains("more may still be loading"), "{}", run.stdout);
    assert!(!first.contains("nothing new loaded"), "{}", run.stdout);
}

#[test]
fn a_loader_still_working_after_the_waits_is_not_called_the_end() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/spinner")]);
    assert_unsure(&tabref.ok(&["scroll", "down", "--px", "20000"]));
}

#[test]
fn a_request_slower_than_the_waits_is_not_called_the_end() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/slow")]);
    assert_unsure(&tabref.ok(&["scroll", "down", "--px", "20000"]));
}

#[test]
fn a_request_started_before_the_scroll_is_not_called_the_end() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/pending")]);
    tabref.ok(&["js", "start(); 1"]);
    assert_unsure(&tabref.ok(&["scroll", "down", "--px", "20000"]));
}

#[test]
fn a_tab_that_draws_no_frames_is_not_called_the_end() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/more")]);
    tabref.ok(&["js", "window.requestAnimationFrame = () => 0; 1"]);
    let run = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert_eq!(
        first_line(&run),
        "scrolled down 20000px; more may still be loading (tab not showing)"
    );
    tabref.ok(&[
        "js",
        "Object.defineProperty(document, 'visibilityState', { get: () => 'hidden' }); 1",
    ]);
    let stuck = tabref.ok(&["scroll", "down"]);
    assert_eq!(
        first_line(&stuck),
        "did not move: at the bottom; more may still be loading (tab not showing)"
    );
}

#[test]
fn a_page_that_loads_through_a_button_says_only_that_nothing_new_loaded() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/more")]);
    let run = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert_eq!(
        first_line(&run),
        "scrolled down 20000px; at the bottom: nothing new loaded"
    );
    let stuck = tabref.ok(&["scroll", "down"]);
    assert_eq!(
        first_line(&stuck),
        "did not move: at the bottom, nothing new loaded"
    );
}
