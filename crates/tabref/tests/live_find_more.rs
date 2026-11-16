//! Live runs of `find` with no match on pages that scroll further: a lazy
//! window list, a lazy scroll container, a long static page scrolled to
//! its end and a page that fits the window, and `find` inside `do`, which
//! must print the same match lines as `tabref find`. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_find_more -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::lazy_site::{batch, lazy_site, rows};
use common::{Harness, harness, live};

const WINDOW_LIST: &str = r#"<!doctype html><title>Ledger</title><style>li{height:40px}</style>
<h1>Ledger</h1><ol id=rows>ROWS</ol><p id=tail>Fetching entries</p>
<script>
let next = 30, busy = false, done = false;
const rows = document.getElementById('rows'), tail = document.getElementById('tail');
function load() {
  if (busy || done) return;
  busy = true;
  fetch('/entries?name=Entry&total=70&delay=100&from=' + next).then(r => r.json()).then(page => {
    for (const text of page.items) { const li = document.createElement('li'); li.innerHTML = '<a></a> in Ledger'; li.firstChild.textContent = text; li.firstChild.href = '/entry/' + text.split(' ')[1]; rows.appendChild(li); }
    next += page.items.length; done = page.last; busy = false;
    tail.textContent = done ? 'No more entries' : 'Fetching entries';
  });
}
new IntersectionObserver(seen => { if (seen[0].isIntersecting) load(); }).observe(tail);
</script>"#;

const BOX_LIST: &str = r#"<!doctype html><title>Inbox</title>
<style>html,body{margin:0;height:100%;overflow:hidden} #box{height:320px;overflow:auto} .card{height:48px}</style>
<div id=box><div id=cards>ROWS</div><div id=spin>More below</div></div>"#;

const SHORT: &str = r#"<!doctype html><title>Note</title><h1>Note</h1><p>One short paragraph.</p>"#;

const LONG: &str = r#"<!doctype html><title>Essay</title><style>p{height:60px}</style>
<h1>Essay</h1>ROWS<footer>The end of the essay</footer>"#;

fn site() -> (Harness, String) {
    let base = lazy_site(vec![
        (
            "/ledger",
            WINDOW_LIST.replace("ROWS", &rows("li", "Entry", 30)),
        ),
        (
            "/inbox",
            BOX_LIST.replace("ROWS", &rows("div", "Message", 20)),
        ),
        ("/note", SHORT.to_owned()),
        ("/essay", LONG.replace("ROWS", &rows("p", "Paragraph", 60))),
    ]);
    (harness("find-more"), base)
}

#[test]
fn no_match_on_a_page_that_scrolls_further_names_the_scroll_loop() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/ledger")]);
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
    tabref.ok(&["open", &format!("{base}/inbox")]);
    let run = tabref.ok(&["find", "Message 40"]);
    let line = run.stdout.trim();
    assert!(
        line.contains("a list on the page scrolls further"),
        "{line}"
    );
    assert!(line.contains("`scroll down \"Message 20\"`"), "{line}");
    let scrolled = tabref.ok(&["scroll", "down", "Message 20"]);
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
    tabref.ok(&["open", &format!("{base}/ledger")]);
    let stepped = batch(
        &tabref,
        &format!("{}find Entry 4\n", "scroll down\n".repeat(4)),
    );
    let alone = tabref.ok(&["find", "Entry 4"]);
    for line in alone.stdout.lines().skip(1).take(10) {
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
