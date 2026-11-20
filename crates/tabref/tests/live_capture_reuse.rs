//! Live runs of the daemon reusing the last capture: a `find`, `snap` or
//! `read` after an `open` of an unchanged page uses the open's snapshot,
//! and any change since (a DOM mutation, a value set by script, a shadow
//! root attached to an existing element, a hover, a closed shadow root it
//! cannot watch) makes it capture again. In-process runs never reuse.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_capture_reuse`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::Command;
use std::time::Duration;

use common::{Harness, Run, live, serve};

const STILL: &str = r#"<!doctype html><html><head><title>Still</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><label>Name <input id="n" value="x"></label>
<h2>More</h2><p>Beta words.</p></body></html>"#;

const LATE: &str = r#"<!doctype html><html><head><title>Late</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p>
<script>setTimeout(() => { const p = document.createElement('p'); p.textContent = 'Late needle words'; document.body.append(p); }, 400);</script>
</body></html>"#;

const VALUE: &str = r#"<!doctype html><html><head><title>Value</title></head><body>
<h1>Guide</h1><label>Name <input id="n" value="before"></label><p>Alpha needle text.</p>
<script>setTimeout(() => { document.getElementById('n').value = 'after'; }, 400);</script>
</body></html>"#;

const SHADOW: &str = r#"<!doctype html><html><head><title>Shadow</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><div id="host"></div>
<script>setTimeout(() => { document.getElementById('host').attachShadow({ mode: 'open' }).innerHTML = '<p>Shadow needle words</p>'; }, 400);</script>
</body></html>"#;

const CLOSED: &str = r#"<!doctype html><html><head><title>Closed</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><div id="host"></div>
<script>document.getElementById('host').attachShadow({ mode: 'closed' }).innerHTML = '<p>Closed needle words</p>';</script>
</body></html>"#;

fn run(tabref: &Harness, args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(["--cdp", &tabref.browser.port.to_string(), "--json"])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env_remove("TABREF_NO_DAEMON");
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().unwrap();
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

fn opened(tabref: &Harness, html: &'static str) -> String {
    let port = serve(&[("/", html)]);
    snapshot_id(&run(
        tabref,
        &["open", &format!("http://127.0.0.1:{port}/")],
        &[],
    ))
}

fn stop(tabref: &Harness) {
    let _ = run(tabref, &["daemon", "stop"], &[]);
}

#[test]
fn find_snap_and_read_reuse_the_capture_of_an_unchanged_page() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-still");
    opened(&tabref, STILL);
    let mut snapshots = Vec::new();
    for _ in 0..5 {
        let kept = snapshot_id(&run(&tabref, &["snap"], &[]));
        let found = run(&tabref, &["find", "needle"], &[]);
        assert!(
            found.stdout.contains("Alpha needle text"),
            "{}",
            found.stdout
        );
        let snapped = run(&tabref, &["snap", "--from", "2"], &[]);
        let read = run(&tabref, &["read", "--section", "More"], &[]);
        assert!(read.stdout.contains("Beta words"), "{}", read.stdout);
        let found_again = run(&tabref, &["find", "Beta"], &[]);
        let ids = [
            kept,
            snapshot_id(&found),
            snapshot_id(&snapped),
            snapshot_id(&found_again),
        ];
        if ids.iter().all(|id| id == &ids[0]) {
            stop(&tabref);
            return;
        }
        snapshots.push(ids);
    }
    stop(&tabref);
    panic!(
        "the watch can time out under load, but no complete sequence reused a capture: {snapshots:?}"
    );
}

#[test]
fn a_page_that_changed_since_is_captured_again() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-changed");
    for (html, needle) in [
        (LATE, "Late needle"),
        (SHADOW, "Shadow needle"),
        (VALUE, "Alpha needle"),
    ] {
        let opened = opened(&tabref, html);
        std::thread::sleep(Duration::from_millis(900));
        let found = run(&tabref, &["find", needle], &[]);
        assert_ne!(
            snapshot_id(&found),
            opened,
            "{needle}: reused a stale capture"
        );
        assert!(found.stdout.contains(needle), "{needle}: {}", found.stdout);
    }
    let opened = opened(&tabref, VALUE);
    std::thread::sleep(Duration::from_millis(900));
    let snapped = run(&tabref, &["snap"], &[]);
    assert_ne!(snapshot_id(&snapped), opened);
    assert!(snapped.stdout.contains("after"), "{}", snapped.stdout);
    stop(&tabref);
}

#[test]
fn any_other_command_or_an_unwatchable_page_captures_again() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-other");
    let opened_still = opened(&tabref, STILL);
    run(&tabref, &["hover", "Name"], &[]);
    let found = run(&tabref, &["find", "needle"], &[]);
    assert_ne!(snapshot_id(&found), opened_still, "kept across a hover");
    let opened_closed = opened(&tabref, CLOSED);
    let found = run(&tabref, &["find", "needle"], &[]);
    assert_ne!(
        snapshot_id(&found),
        opened_closed,
        "kept a closed shadow root"
    );
    stop(&tabref);
}

#[test]
fn an_in_process_run_never_reuses() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-local");
    let local = [("TABREF_NO_DAEMON", "1")];
    let port = serve(&[("/", STILL)]);
    let url = format!("http://127.0.0.1:{port}/");
    let opened = snapshot_id(&run(&tabref, &["open", &url], &local));
    let found = run(&tabref, &["find", "needle"], &local);
    assert_ne!(snapshot_id(&found), opened);
}
