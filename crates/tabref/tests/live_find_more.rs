//! Live runs of `find` with no match on pages that scroll further: a lazy
//! window list, a lazy scroll container (alone and on a page that scrolls
//! a little), a long static page and a page that fits the window, and
//! `find` inside `do`, which must print the first lines `tabref find`
//! prints. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_find_more -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::lazy_site::{batch, lazy_site, rows};
use common::{Harness, harness, live};

const WINDOW_LIST: &str = r#"<!doctype html><title>Ledger</title><style>li{height:40px}</style>
<h1>Ledger</h1><ol id=rows></ol><p id=tail>Fetching entries</p>
<script>
let next = 0, busy = false, done = false;
const rows = document.getElementById('rows'), tail = document.getElementById('tail');
function load(count) {
  if (busy || done) return;
  busy = true;
  fetch('/entries?name=Entry&total=70&delay=100&count=' + count + '&from=' + next).then(r => r.json()).then(page => {
    for (const text of page.items) { const li = document.createElement('li'); li.innerHTML = '<a></a> in Ledger'; li.firstChild.textContent = text; li.firstChild.href = '/entry/' + text.split(' ')[1]; rows.appendChild(li); }
    next += page.items.length; done = page.last; busy = false;
    tail.textContent = done ? 'No more entries' : 'Fetching entries';
  });
}
load(30);
new IntersectionObserver(seen => { if (seen[0].isIntersecting && next) load(20); }).observe(tail);
</script>"#;

const BOX_LIST: &str = r#"<!doctype html><title>Inbox</title>
<style>html,body{margin:0;height:100%;overflow:hidden} #box{height:320px;overflow:auto} .card{height:48px}</style>
BOX"#;

const SPLIT: &str = r#"<!doctype html><title>Desk</title>
<style>body{margin:0} #pad{height:calc(100vh - 170px)} #box{height:320px;overflow:auto} .card{height:48px}</style>
<div id=pad>Desk <button>Message 40</button></div>BOX"#;

const BOX: &str = r#"<div id=box><div id=cards></div><div id=spin>More below</div></div>
<script>
const cards = document.getElementById('cards'), box = document.getElementById('box');
let next = 0, busy = false;
function load() {
  if (busy || next >= 60) return;
  busy = true;
  fetch('/entries?name=Message&total=60&delay=50&from=' + next).then(r => r.json()).then(page => {
    for (const text of page.items) {
      const card = document.createElement('div');
      card.className = 'card';
      card.innerHTML = '<a href="/m"></a> <button>Reply</button> <button aria-label="Close">×</button>';
      card.firstChild.textContent = text;
      cards.appendChild(card);
    }
    next += page.items.length; busy = false;
  });
}
load();
box.addEventListener('scroll', () => { if (box.scrollTop + box.clientHeight > box.scrollHeight - 60) load(); });
</script>"#;

const SHORT: &str = r#"<!doctype html><title>Note</title><h1>Note</h1><p>One short paragraph.</p>"#;

const LONG: &str = r#"<!doctype html><title>Essay</title><style>p{height:60px}</style>
<h1>Essay</h1>ROWS<footer>The end of the essay</footer>
<script>
window.analytics = new Promise(resolve => addEventListener('scroll',
  () => fetch('/analytics').then(reply => reply.text()).then(resolve), {once: true}));
</script>"#;

fn site() -> (Harness, String) {
    let base = lazy_site(vec![
        ("/ledger", WINDOW_LIST.to_owned()),
        ("/inbox", BOX_LIST.replace("BOX", BOX)),
        ("/desk", SPLIT.replace("BOX", BOX)),
        ("/note", SHORT.to_owned()),
        ("/essay", LONG.replace("ROWS", &rows("p", "Paragraph", 60))),
        ("/analytics", "received".to_owned()),
    ]);
    (harness("find-more"), base)
}

#[test]
fn no_match_on_a_page_that_scrolls_further_names_the_scroll_loop() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    open_loaded(&tabref, &format!("{base}/ledger"), "Entry 30");
    let before = tabref.ok(&["find", "Entry 65"]);
    assert!(
        !before.stdout.contains("scrolls further"),
        "{}",
        before.stdout
    );
    tabref.ok(&["scroll", "down", "--px", "20000"]);
    batch(&tabref, "expect text \"Entry 50\"\n");
    let run = tabref.ok(&["find", "Entry 65"]);
    let line = run.stdout.trim();
    assert!(line.starts_with("0 matches for \"Entry 65\""), "{line}");
    assert!(line.contains("the page scrolls further"), "{line}");
    assert!(line.contains("scroll down"), "{line}");
    assert!(line.contains("tabref do"), "{line}");
    let words = tabref.ok(&["find", "Entry", "65"]);
    assert!(
        words.stdout.starts_with("0 matches for \"Entry 65\""),
        "{}",
        words.stdout
    );
    let stepped = batch(&tabref, "find Entry 65\n");
    assert!(
        stepped
            .stdout
            .contains("(0 matches; the page scrolls further"),
        "{}",
        stepped.stdout
    );
}

#[test]
fn no_match_in_a_list_that_scrolls_names_a_target_inside_it() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    open_loaded(&tabref, &format!("{base}/inbox"), "Message 20");
    tabref.ok(&["scroll", "down", "Message 20", "--px", "20000"]);
    batch(&tabref, "expect text \"Message 40\"\n");
    let run = tabref.ok(&["find", "Message 55"]);
    let line = run.stdout.trim();
    assert!(
        line.contains("a list on the page scrolls further"),
        "{line}"
    );
    assert!(line.contains("`scroll down \"Message 40\"`"), "{line}");
    open_loaded(&tabref, &format!("{base}/desk"), "Message 20");
    tabref.ok(&["scroll", "down", "Message 20", "--px", "20000"]);
    batch(&tabref, "expect text \"Message 39\"\n");
    let desk = tabref.ok(&["find", "Message 55"]);
    let line = desk.stdout.trim();
    assert!(
        line.contains("; a list on the page scrolls further"),
        "{line}"
    );
    assert!(line.contains("`scroll down \"Message 39\"`"), "{line}");
    let scrolled = tabref.ok(&["scroll", "down", "Message 39"]);
    assert!(
        scrolled.stdout.starts_with("scrolled down"),
        "{}",
        scrolled.stdout
    );
}

#[test]
fn no_match_where_nothing_scrolls_says_nothing_about_scrolling() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/note")]);
    let short = tabref.ok(&["find", "Absent words"]);
    assert!(!short.stdout.contains("scroll"), "{}", short.stdout);
    tabref.ok(&["open", &format!("{base}/essay")]);
    let top = tabref.ok(&["find", "Absent words"]);
    assert!(!top.stdout.contains("scroll"), "{}", top.stdout);
    tabref.ok(&["scroll", "down"]);
    assert!(
        tabref
            .ok(&["js", "window.analytics"])
            .stdout
            .contains("received")
    );
    let after_analytics = tabref.ok(&["find", "Absent words"]);
    assert!(
        !after_analytics.stdout.contains("scrolls further"),
        "{}",
        after_analytics.stdout
    );
    tabref.ok(&["js", "window.scrollTo(0, document.body.scrollHeight)"]);
    let bottom = tabref.ok(&["find", "Absent words"]);
    assert!(!bottom.stdout.contains("scroll"), "{}", bottom.stdout);
    let found = tabref.ok(&["find", "Paragraph 7"]);
    assert!(!found.stdout.contains("scroll"), "{}", found.stdout);
}

#[test]
fn find_in_a_batch_prints_the_lines_find_prints() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    open_loaded(&tabref, &format!("{base}/ledger"), "Entry 30");
    let stepped = batch(
        &tabref,
        &format!("{}find Entry 4\n", "scroll down\n".repeat(4)),
    );
    let alone = tabref.ok(&["find", "Entry 4"]);
    let shown: Vec<&str> = alone.stdout.lines().skip(1).take(5).collect();
    for line in &shown {
        let printed = format!("  {}", strip_refs(line));
        assert!(
            stepped
                .stdout
                .lines()
                .map(strip_refs)
                .any(|step| step == printed),
            "missing {line:?} in\n{}",
            stepped.stdout
        );
    }
    assert!(stepped.stdout.contains("-> /entry/"), "{}", stepped.stdout);
    let printed = stepped
        .stdout
        .lines()
        .filter(|line| line.starts_with("  - "))
        .count();
    assert_eq!(printed, 5, "{}", stepped.stdout);
    assert!(
        stepped.stdout.contains("more: tabref find \"Entry 4\""),
        "{}",
        stepped.stdout
    );
    let only_find = batch(&tabref, "find Entry 4\n");
    assert!(
        only_find.stdout.lines().count() <= 10,
        "{}",
        only_find.stdout
    );
    let json = tabref.ok(&["--json", "do", "find Entry 4"]).json();
    let result = &json["data"]["steps"][0]["data"];
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 6, "{json}");
    let cli = tabref.ok(&["--json", "find", "Entry 4"]).json();
    assert_eq!(result["count"], cli["data"]["count"], "{json}");
    assert!(result["count"].as_u64().unwrap() > 5, "{json}");
    let cli_matches = cli["data"]["matches"].as_array().unwrap();
    for (step, standalone) in matches.iter().take(5).zip(cli_matches) {
        assert_eq!(
            strip_refs(step.as_str().unwrap()),
            strip_refs(standalone.as_str().unwrap()),
        );
    }
    assert!(matches[5].as_str().unwrap().starts_with("… "), "{json}");
    assert!(
        matches
            .iter()
            .take(5)
            .all(|line| line.as_str().is_some_and(|s| s.contains("-> /entry/"))),
        "{json}"
    );
}

/// Opens `url` and waits for `text`, which the page fetches after load.
fn open_loaded(tabref: &Harness, url: &str, text: &str) {
    tabref.ok(&["open", url]);
    let waited = batch(tabref, &format!("expect text \"{text}\"\n"));
    assert!(waited.ok, "{}{}", waited.stdout, waited.stderr);
}

fn strip_refs(line: &str) -> String {
    line.split(' ')
        .map(|word| {
            let bare = word.strip_prefix('[').unwrap_or(word);
            let id = bare.rsplit(':').next().unwrap_or(bare);
            let is_ref = word.starts_with('[')
                && id.len() > 1
                && id.starts_with('e')
                && id[1..].chars().all(|c| c.is_ascii_digit());
            if is_ref {
                "[".to_owned()
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
