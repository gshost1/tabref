//! Live runs of `scroll` where tabref cannot know the list has ended: a
//! loader still working after the waits, a slow request, a request started
//! before the command, a tab that draws no frames, and a page that loads
//! more only through a button. None may say it is at the bottom with
//! nothing new loaded unless that is all it knows. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_scroll_doubt`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Duration;

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
  fetch('/entries?name=Late&total=40&from=30&delay=4500').then(r => r.json()).then(page => {
    for (const text of page.items) { const li = document.createElement('li'); li.textContent = text; document.getElementById('rows').appendChild(li); }
  });
}).observe(document.getElementById('tail'));
</script>"#;

const IN_FLIGHT: &str = r#"<!doctype html><title>Pending</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><p id=wait>Idle</p>
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

const NAVIGATES: &str = r#"<!doctype html><title>Jump</title><style>li{height:40px}</style>
<ol id=rows>ROWS</ol><p id=tail></p>
<script>
new IntersectionObserver(seen => { if (seen[0].isIntersecting) location.href = '/more'; })
  .observe(document.getElementById('tail'));
</script>"#;

fn site() -> (Harness, String) {
    let base = lazy_site(vec![
        (
            "/spinner",
            SPINNER.replace("ROWS", &rows("li", "Entry", 30)),
        ),
        (
            "/slow",
            SLOW_FETCH.replace("ROWS", &rows("li", "Entry", 30)),
        ),
        (
            "/pending",
            IN_FLIGHT.replace("ROWS", &rows("li", "Entry", 30)),
        ),
        ("/more", LOAD_MORE.replace("ROWS", &rows("li", "Item", 30))),
        ("/jump", NAVIGATES.replace("ROWS", &rows("li", "Entry", 30))),
    ]);
    (harness("scroll-doubt"), base)
}

fn first_line(run: &Run) -> &str {
    run.stdout.lines().next().unwrap_or_default()
}

fn assert_unsure(run: &Run) {
    let first = first_line(run);
    assert!(first.starts_with("scrolled down"), "{}", run.stdout);
    assert!(
        first.contains("more may still be loading"),
        "{}",
        run.stdout
    );
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
    let next = tabref.ok(&["--json", "scroll", "down"]);
    let data = next.json();
    assert_eq!(data["data"]["loading"], true, "{data}");
    assert_eq!(data["data"]["moved"], false, "{data}");
    assert_eq!(data["data"]["at_end"], true, "{data}");
    assert_eq!(
        first_line(&tabref.ok(&["scroll", "down"])),
        "did not move: at the bottom; more may still be loading"
    );
}

#[test]
fn a_cross_site_fetch_is_followed_and_reports_the_loaded_rows() {
    if !live() {
        return;
    }
    let remote = lazy_site(vec![]).replace("127.0.0.1", "localhost");
    let page = SLOW_FETCH
        .replace("ROWS", &rows("li", "Entry", 30))
        .replace("fetch('/entries", &format!("fetch('{remote}/entries"))
        .replace("delay=4500", "delay=950");
    let base = lazy_site(vec![("/feed", page)]);
    let tabref = harness("scroll-cross-site");
    tabref.ok(&["open", &format!("{base}/feed")]);
    tabref.ok(&[
        "js",
        "Object.defineProperty(performance, 'timeOrigin', {value: Date.now() + 60000}); 1",
    ]);
    let run = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert!(
        first_line(&run).contains("10 new items loaded"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_tab_with_an_old_request_tracker_can_still_scroll() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/more")]);
    tabref.ok(&[
        "js",
        "window[Symbol.for('tabref.scroll-requests')] = {pending: 0}; 1",
    ]);
    let moved = tabref.ok(&["--json", "scroll", "down", "--px", "20000"]);
    assert_eq!(moved.json()["data"]["at_end"], true, "{}", moved.stdout);
    let at_end = tabref.ok(&["--json", "scroll", "down"]);
    assert_eq!(at_end.json()["data"]["moved"], false, "{}", at_end.stdout);
    assert_eq!(at_end.json()["data"]["loading"], false, "{}", at_end.stdout);
}

#[test]
fn a_list_in_an_open_shadow_root_reports_the_rows_it_adds() {
    if !live() {
        return;
    }
    let page = r#"<!doctype html><title>Shadow feed</title><div id=feed></div>
<script>
const root = document.getElementById('feed').attachShadow({mode: 'open'});
root.innerHTML = '<style>li{height:40px}</style><ol id=rows>ROWS</ol><p id=tail>More</p>';
let started = false;
new IntersectionObserver(seen => {
  if (!seen[0].isIntersecting || started) return;
  started = true;
  setTimeout(() => root.getElementById('rows').insertAdjacentHTML('beforeend', '<li>Shadow entry</li>'), 100);
}).observe(root.getElementById('tail'));
</script>"#.replace("ROWS", &rows("li", "Entry", 30));
    let base = lazy_site(vec![("/shadow", page)]);
    let tabref = harness("scroll-shadow");
    tabref.ok(&["open", &format!("{base}/shadow")]);
    let run = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert!(
        first_line(&run).contains("1 new item loaded"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("Shadow entry"), "{}", run.stdout);
}

#[test]
fn a_scroller_inside_a_shadow_root_reports_existing_and_late_shadow_rows() {
    if !live() {
        return;
    }
    let page = r#"<!doctype html><title>Shadow scroller</title><div id=feed></div>
<script>
const root = document.getElementById('feed').attachShadow({mode: 'open'});
root.innerHTML = '<style>#scroller{height:240px;overflow:auto} li{height:40px}</style>'
  + '<div id=scroller><ol id=rows>ROWS</ol><div id=late></div><button>Scroll marker</button></div>';
const scroller = root.getElementById('scroller');
scroller.addEventListener('scroll', () => {
  setTimeout(() => {
    root.getElementById('rows').insertAdjacentHTML('beforeend', '<li>Existing shadow row</li>');
    const late = root.getElementById('late').attachShadow({mode:'open'});
    late.innerHTML = '<p>Late shadow row</p>';
  }, 75);
}, {once:true});
</script>"#
        .replace("ROWS", &rows("li", "Entry", 30));
    let base = lazy_site(vec![("/scroller", page)]);
    let tabref = harness("scroll-shadow-scroller");
    tabref.ok(&["open", &format!("{base}/scroller")]);
    let run = tabref.ok(&["scroll", "down", "Scroll marker", "--px", "20000"]);
    assert!(
        first_line(&run).contains("2 new items loaded"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("+ Existing shadow row"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("+ Late shadow row"), "{}", run.stdout);
}

#[test]
fn a_long_poll_stops_counting_as_loading_at_the_bottom() {
    if !live() {
        return;
    }
    let page = r#"<!doctype html><title>Long poll</title><style>li{height:40px}</style>
<ol>ROWS</ol>
<script>
function startPoll() {
  window.poll = new XMLHttpRequest();
  poll.open('GET', '/entries?delay=60000');
  poll.send();
}
</script>"#
        .replace("ROWS", &rows("li", "Entry", 30));
    let base = lazy_site(vec![("/long-poll", page)]);
    let tabref = harness("scroll-long-poll");
    tabref.ok(&["open", &format!("{base}/long-poll")]);
    tabref.ok(&["scroll", "down", "--px", "20000"]);
    tabref.ok(&["js", "startPoll(); 1"]);

    let pending = tabref.ok(&["--json", "scroll", "down"]).json();
    assert_eq!(pending["data"]["loading"], true, "{pending}");
    std::thread::sleep(Duration::from_millis(3_100));
    let settled = tabref.ok(&["--json", "scroll", "down"]).json();
    assert_eq!(settled["data"]["loading"], false, "{settled}");
    assert_eq!(settled["data"]["at_end"], true, "{settled}");
    let poll = tabref.ok(&["js", "poll.readyState"]).stdout;
    assert_eq!(poll.trim(), "1", "{poll}");
}

#[test]
fn a_rejection_in_top_level_await_is_reported_as_a_rejection() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/more")]);
    let run = tabref.run(&[
        "--json",
        "js",
        "await Promise.reject(new Error('quota gone'))",
    ]);
    assert!(!run.ok, "{}", run.stdout);
    assert_eq!(
        run.json()["error"]["details"]["kind"],
        "js_rejected",
        "{}",
        run.stdout
    );
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

#[test]
fn a_page_that_navigates_away_mid_watch_still_reports_the_scroll() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/jump")]);
    let run = tabref.ok(&["scroll", "down", "--px", "20000"]);
    assert_eq!(
        first_line(&run),
        "scrolled down 20000px; could not check what loaded"
    );
}
