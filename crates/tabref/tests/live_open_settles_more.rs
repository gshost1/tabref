//! More live runs of what `open` waits for after `load`: a navigation the
//! page starts during the wait, a short `--timeout`, a run without the
//! daemon, a fetch that never ends on a full page, the `do` open step, and
//! leaving a page whose main thread is busy for a page of another site
//! (a page of the same site shares the busy renderer, so it cannot load).
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_open_settles_more`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::slow_serve::{run, run_as, run_env, serve};
use common::{Harness, Run, live};

const STATIC: &str = r#"<!doctype html><html><head><title>Still</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p></body></html>"#;

const LATE: &str = r#"<!doctype html><html><head><title>Late</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><div id="slot"></div>
<script>addEventListener('load', () => fetch('/data?ms=150').then(r => r.text()).then(t => {
  const p = document.createElement('p'); p.textContent = 'Late needle ' + t; document.getElementById('slot').append(p);
}));</script></body></html>"#;

const LATE_FULL: &str = r#"<!doctype html><html><head><title>Late</title></head><body>
<h1>Guide</h1><p>Alpha needle text with enough words to be a page of its own and not near-empty at all.</p>
<p>More words follow here so the page has real content, about two hundred characters of it, read at once.</p>
<p>A third paragraph keeps it well past the near-empty line, whatever the browser counts as rendered text.</p>
<div id="slot"></div>
<script>addEventListener('load', () => fetch('/data?ms=150').then(r => r.text()).then(t => {
  const p = document.createElement('p'); p.textContent = 'Late needle ' + t; document.getElementById('slot').append(p);
}));</script></body></html>"#;

const SPA: &str = r#"<!doctype html><html><head><title>App</title></head><body>
<header><nav><a href="/">Home</a> <input placeholder="Search"> <button>Go</button></nav></header>
<main id="root"></main>
<script>fetch('/data?ms=600').then(r => r.text()).then(t => {
  document.getElementById('root').innerHTML = '<h1>Dashboard</h1><p>Rendered report ' + t + '</p>';
});</script></body></html>"#;

const REDIRECT: &str = r#"<!doctype html><html><head><title>First</title></head><body>
<h1>First page</h1><p>Alpha needle text.</p>
<script>addEventListener('load', () => fetch('/data?ms=100').then(() => { location.href = '/second'; }));</script>
</body></html>"#;

const SECOND: &str = r#"<!doctype html><html><head><title>Second</title></head><body>
<h1>Second page</h1><p>Beta words.</p></body></html>"#;

const ENDLESS: &str = r#"<!doctype html><html><head><title>Endless</title></head><body>
<h1>Guide</h1><p>Alpha needle text with enough words to be a page of its own and not near-empty at all.</p>
<p>More words follow here so the page has real content, about two hundred characters of it, read at once.</p>
<p>A third paragraph keeps it well past the near-empty line, whatever the browser counts as rendered text.</p>
<script>addEventListener('load', () => fetch('/data?ms=20000'));</script></body></html>"#;

const BUSY: &str = r#"<!doctype html><html><head><title>Busy</title></head><body>
<h1>Busy page</h1><p>Alpha needle text.</p>
<script>addEventListener('load', () => setTimeout(() => { const t = Date.now(); while (Date.now() - t < 10000) {} }, 1200));</script>
</body></html>"#;

fn url(html: &'static str) -> String {
    let port = serve(&[("/", html), ("/second", SECOND)]);
    format!("http://127.0.0.1:{port}/")
}

fn timed(run: impl FnOnce() -> Run) -> (Run, Duration) {
    let started = Instant::now();
    let done = run();
    (done, started.elapsed())
}

fn stop(tabref: &Harness) {
    let _ = run(tabref, &["daemon", "stop"]);
}

#[test]
fn a_navigation_during_the_wait_ends_it() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-redirect");
    let (opened, took) = timed(|| run_as(&tabref, &[], &["open", &url(REDIRECT)]));
    let first = opened.stdout.contains("First page");
    let second = opened.stdout.contains("Second page");
    eprintln!("redirect: took {took:?}, first page {first}, second page {second}");
    assert!(first || second, "{}", opened.stdout);
    assert!(took < Duration::from_millis(3_000), "open took {took:?}");
    stop(&tabref);
}

#[test]
fn a_short_timeout_still_gets_a_view_in_time() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-short");
    run(&tabref, &["open", &url(STATIC)]);
    for html in [LATE, SPA] {
        let (opened, took) = timed(|| run(&tabref, &["--timeout", "1500", "open", &url(html)]));
        assert!(
            opened.json()["data"]["lines"].to_string().contains("Guide")
                || opened.json()["data"]["lines"]
                    .to_string()
                    .contains("Search")
                || opened.json()["data"]["title"] == "App",
            "{}",
            opened.stdout
        );
        assert!(took < Duration::from_millis(1_500), "open took {took:?}");
    }
    stop(&tabref);
}

#[test]
fn without_the_daemon_only_a_near_empty_page_is_waited_for() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-local");
    let local = [("TABREF_NO_DAEMON", "1")];
    run_env(&tabref, &["--json"], &["open", &url(STATIC)], &local);
    let (_, base) = timed(|| run_env(&tabref, &["--json"], &["open", &url(STATIC)], &local));
    let (_, late) = timed(|| run_env(&tabref, &["--json"], &["open", &url(LATE_FULL)], &local));
    assert!(
        late < base + Duration::from_millis(120),
        "late page took {late:?}, a static one {base:?}"
    );
    let app = run_env(&tabref, &[], &["open", &url(SPA)], &local);
    assert!(app.stdout.contains("Rendered report"), "{}", app.stdout);
}

#[test]
fn a_fetch_that_never_ends_on_a_full_page_costs_the_bound() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-endless");
    run(&tabref, &["open", &url(STATIC)]);
    let (_, base) = timed(|| run(&tabref, &["open", &url(STATIC)]));
    let (opened, took) = timed(|| run(&tabref, &["open", &url(ENDLESS)]));
    assert!(
        opened.json()["data"]["still_rendering"].is_null(),
        "{}",
        opened.stdout
    );
    assert!(
        took < base + Duration::from_millis(550),
        "open took {took:?}, a warm static open {base:?}"
    );
    stop(&tabref);
}

#[test]
fn the_do_open_step_lets_an_app_render() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-do");
    let steps = format!("open {}; find \"Rendered report\"", url(SPA));
    let done = run_as(&tabref, &[], &["do", &steps]);
    assert!(
        done.stdout.contains("Rendered report fetched words"),
        "{}",
        done.stdout
    );
    stop(&tabref);
}

#[test]
fn leaving_a_page_whose_main_thread_is_busy_keeps_the_time() {
    if !live() {
        return;
    }
    let tabref = common::harness("settle-hung");
    run(&tabref, &["open", &url(BUSY)]);
    std::thread::sleep(Duration::from_millis(1_500));
    let other = url(SECOND).replace("127.0.0.1", "localhost");
    let (opened, took) = timed(|| run_as(&tabref, &[], &["--timeout", "5000", "open", &other]));
    assert!(opened.stdout.contains("Second page"), "{}", opened.stdout);
    assert!(took < Duration::from_millis(3_000), "open took {took:?}");
    stop(&tabref);
}
