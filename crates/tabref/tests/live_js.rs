//! Live runs of `js` against headless Chrome: plain values, a promise the
//! expression returns or awaits, one that rejects, and one that never
//! settles. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_js -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::{Harness, harness, live, serve};

const PAGE: &str = "<!doctype html><title>Scripted</title><h1>Scripted</h1>";

fn page() -> Harness {
    let port = serve(&[("/", PAGE), ("/data", r#"{"ok":true}"#)]);
    let tabref = harness("js");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    tabref
}

#[test]
fn plain_values_print_as_before() {
    if !live() {
        return;
    }
    let tabref = page();
    assert_eq!(tabref.ok(&["js", "1 + 1"]).stdout.trim(), "2");
    assert_eq!(
        tabref.ok(&["js", "document.title"]).stdout.trim(),
        "Scripted"
    );
    assert_eq!(
        tabref.ok(&["js", "({ a: [1, 'b'] })"]).stdout.trim(),
        r#"{"a":[1,"b"]}"#
    );
    assert_eq!(tabref.ok(&["js", "undefined"]).stdout.trim(), "null");
    assert_eq!(
        tabref
            .ok(&["js", "document.querySelector('h1')"])
            .stdout
            .trim(),
        "{}"
    );
}

#[test]
fn a_returned_promise_prints_what_it_resolved_to() {
    if !live() {
        return;
    }
    let tabref = page();
    let timer = tabref.ok(&["js", "new Promise(r => setTimeout(() => r(42), 200))"]);
    assert_eq!(timer.stdout.trim(), "42");
    let fetched = tabref.ok(&["js", "fetch('/data').then(r => r.status)"]);
    assert_eq!(fetched.stdout.trim(), "200");
    let iife = tabref.ok(&[
        "js",
        "(async () => { await new Promise(r => setTimeout(r, 50)); return { rows: 3 }; })()",
    ]);
    assert_eq!(iife.stdout.trim(), r#"{"rows":3}"#);
    let awaited = tabref.ok(&["js", "await Promise.resolve('done')"]);
    assert_eq!(awaited.stdout.trim(), "done");
}

#[test]
fn a_rejected_promise_fails_with_its_reason() {
    if !live() {
        return;
    }
    let tabref = page();
    let run = tabref.run(&["js", "Promise.reject(new Error('quota gone'))"]);
    assert!(!run.ok, "{}", run.stdout);
    assert!(run.stderr.contains("rejected"), "{}", run.stderr);
    assert!(run.stderr.contains("quota gone"), "{}", run.stderr);
}

#[test]
fn a_promise_that_never_settles_times_out_plainly() {
    if !live() {
        return;
    }
    let tabref = page();
    let started = Instant::now();
    let run = tabref.run(&["--timeout", "1500", "js", "new Promise(() => {})"]);
    assert!(started.elapsed() < Duration::from_millis(6_000));
    assert!(!run.ok, "{}", run.stdout);
    assert!(run.stderr.starts_with("error TIMEOUT"), "{}", run.stderr);
    assert!(run.stderr.contains("did not settle"), "{}", run.stderr);
    assert_eq!(tabref.ok(&["js", "6 * 7"]).stdout.trim(), "42");
}
