//! Live runs of `scroll` against headless Chrome and lists that load more
//! as they are scrolled: a window list fed by an `IntersectionObserver`
//! sentinel and a delayed fetch, a scroll container fed by a debounced
//! scroll handler, a static page, and the end of each list. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_scroll -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::lazy_site::{batch, lazy_site, rows};
use common::{Harness, harness, live};

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

const FEED: &str = r#"<!doctype html><title>Feed</title><style>article{height:130px}</style>
<h1>Feed</h1><div id=feed>ROWS</div><div id=end></div>
<script>
let next = 10;
new IntersectionObserver(seen => {
  if (!seen[0].isIntersecting || next >= 20) return;
  fetch('/entries?name=Story&total=20&from=' + next).then(r => r.json()).then(page => {
    const section = document.createElement('section');
    for (const text of page.items) {
      const article = document.createElement('article');
      article.innerHTML = '<h3></h3><p></p>';
      article.querySelector('h3').textContent = text;
      article.querySelector('p').textContent = 'by ' + text.replace('Story', 'Author');
      section.appendChild(article);
    }
    next += page.items.length;
    document.getElementById('feed').appendChild(section);
  });
}, { rootMargin: '300px' }).observe(document.getElementById('end'));
</script>"#;

const STATIC: &str = r#"<!doctype html><title>Essay</title><style>p{height:60px}</style>
<h1>Essay</h1>ROWS<footer>The end of the essay</footer>"#;

fn site() -> (Harness, String) {
    let base = lazy_site(vec![
        (
            "/ledger",
            SENTINEL.replace("ROWS", &rows("li", "Entry", 30)),
        ),
        (
            "/inbox",
            CONTAINER.replace("ROWS", &rows("div", "Message", 20)),
        ),
        ("/feed", FEED.replace("ROWS", &rows("article", "Story", 10))),
        (
            "/essay",
            STATIC.replace("ROWS", &rows("p", "Paragraph", 60)),
        ),
    ]);
    (harness("scroll"), base)
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
        last.contains("at the bottom, nothing new loaded"),
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
        last.stdout.contains("at the bottom: nothing new loaded"),
        "{}",
        last.stdout
    );
    let end = tabref.ok(&["scroll", "down", "--px", "2000"]);
    let first = end.stdout.lines().next().unwrap_or_default();
    assert!(first.starts_with("did not move"), "{}", end.stdout);
    assert!(first.contains("nothing new loaded"), "{}", end.stdout);
}

#[test]
fn cards_added_in_one_wrapper_count_as_items_with_all_their_lines() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/feed")]);
    let run = tabref.ok(&["scroll", "down"]);
    let first = run.stdout.lines().next().unwrap_or_default();
    assert_eq!(
        first,
        r#"scrolled down 600px: 10 new items loaded ("Story 11" to "Story 20")"#
    );
    assert!(run.stdout.contains("+ by Author 11"), "{}", run.stdout);
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
        end.stdout.contains("at the bottom: nothing new loaded"),
        "{}",
        end.stdout
    );
    let stuck = tabref.ok(&["scroll", "down"]);
    assert_eq!(
        stuck.stdout.trim(),
        "did not move: at the bottom, nothing new loaded"
    );
    let back = tabref.ok(&["scroll", "up"]);
    assert_eq!(back.stdout.trim(), "scrolled up 600px");
}
