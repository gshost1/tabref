//! More live runs of the daemon's capture reuse: a page whose CSS
//! animation or transition still runs, an address changed by `pushState`
//! or the hash, state no mutation shows (a multi-select's selection, a
//! checkbox's `indeterminate`, focus inside a same-origin frame), a `js`
//! run, and an in-process command between two daemon ones all make the
//! next `find` or `snap` capture again.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_capture_reuse_more`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::Command;
use std::time::Duration;

use common::{Harness, Run, live, serve};

const STILL: &str = r#"<!doctype html><html><head><title>Still</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><label>Name <input id="n" value="x"></label>
<h2>More</h2><p>Beta words.</p></body></html>"#;

const ANIMATED: &str = r#"<!doctype html><html><head><title>Menu</title><style>
.item { animation: wait 1500ms; }
@keyframes wait { 0%, 100% { visibility: hidden; } }
</style></head><body><h1>Guide</h1><p>Alpha needle text.</p>
<ul><li class="item"><a href="/settings">Settings entry</a></li></ul></body></html>"#;

const TRANSITION: &str = r#"<!doctype html><html><head><title>Menu</title><style>
.item { visibility: hidden; transition: visibility 0s linear 1500ms; }
.open .item { visibility: visible; }
</style></head><body><h1>Guide</h1><p>Alpha needle text.</p>
<ul id="menu"><li class="item"><a href="/settings">Settings entry</a></li></ul>
<script>addEventListener('load', () => requestAnimationFrame(() => document.getElementById('menu').classList.add('open')));</script>
</body></html>"#;

const PUSHED: &str = r#"<!doctype html><html><head><title>Pushed</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p>
<script>setTimeout(() => history.pushState({}, '', '/moved'), 400);</script></body></html>"#;

const HASHED: &str = r#"<!doctype html><html><head><title>Hashed</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p>
<script>setTimeout(() => { location.hash = 'later'; }, 400);</script></body></html>"#;

const MULTI: &str = r#"<!doctype html><html><head><title>Multi</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><label>Sizes <select id="s" multiple>
<option selected>Small</option><option>Medium</option><option>Large</option></select></label>
<script>setTimeout(() => { document.getElementById('s').options[2].selected = true; }, 400);</script>
</body></html>"#;

const MIXED: &str = r#"<!doctype html><html><head><title>Mixed</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p><label><input id="c" type="checkbox"> All rows</label>
<script>setTimeout(() => { document.getElementById('c').indeterminate = true; }, 400);</script>
</body></html>"#;

const FRAMED: &str = r#"<!doctype html><html><head><title>Framed</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p>
<iframe id="f" srcdoc="<input id=a aria-label=First><input id=b aria-label=Second>"></iframe>
<script>
addEventListener('load', () => document.getElementById('f').contentDocument.getElementById('a').focus());
setTimeout(() => document.getElementById('f').contentDocument.getElementById('b').focus(), 600);
</script></body></html>"#;

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

fn open(tabref: &Harness, html: &'static str) -> Run {
    let port = serve(&[("/", html)]);
    run(tabref, &["open", &format!("http://127.0.0.1:{port}/")], &[])
}

fn stop(tabref: &Harness) {
    let _ = run(tabref, &["daemon", "stop"], &[]);
}

#[test]
fn a_running_animation_or_transition_is_never_replayed() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-animated");
    for html in [ANIMATED, TRANSITION] {
        let opened = open(&tabref, html);
        assert!(
            !opened.stdout.contains("Settings entry"),
            "the open came after the animation; the test proves nothing:\n{}",
            opened.stdout
        );
        std::thread::sleep(Duration::from_millis(1_900));
        let snapped = run(&tabref, &["snap"], &[]);
        assert_ne!(snapshot_id(&snapped), snapshot_id(&opened), "replayed");
        assert!(
            snapped.stdout.contains("Settings entry"),
            "{}",
            snapped.stdout
        );
    }
    stop(&tabref);
}

#[test]
fn state_no_mutation_shows_makes_find_capture_again() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-state");
    for (name, html) in [
        ("pushState", PUSHED),
        ("hash", HASHED),
        ("multi-select", MULTI),
        ("indeterminate", MIXED),
        ("frame focus", FRAMED),
    ] {
        let opened = snapshot_id(&open(&tabref, html));
        std::thread::sleep(Duration::from_millis(1_000));
        let found = run(&tabref, &["find", "needle"], &[]);
        assert_ne!(
            snapshot_id(&found),
            opened,
            "{name}: reused a stale capture"
        );
    }
    stop(&tabref);
}

#[test]
fn js_and_in_process_commands_drop_the_kept_capture() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-js");
    let opened = snapshot_id(&open(&tabref, STILL));
    run(
        &tabref,
        &[
            "js",
            "{ const s = new CSSStyleSheet(); s.replaceSync('p { display: none }'); document.adoptedStyleSheets = [s]; 1 }",
        ],
        &[],
    );
    let found = run(&tabref, &["find", "needle"], &[]);
    assert_ne!(snapshot_id(&found), opened, "kept across js");
    let opened = snapshot_id(&open(&tabref, STILL));
    run(&tabref, &["hover", "Name"], &[("TABREF_NO_DAEMON", "1")]);
    let found = run(&tabref, &["find", "needle"], &[]);
    assert_ne!(
        snapshot_id(&found),
        opened,
        "kept across an in-process hover"
    );
    stop(&tabref);
}
