//! More live runs of the daemon's capture reuse: a page whose CSS
//! animation or transition still runs, an address changed by `pushState`
//! or the hash, state no mutation shows (a multi-select's selection, a
//! checkbox's `indeterminate`, focus inside a same-origin frame), a `js`
//! run, and an in-process command between two daemon ones all make the
//! next `find` or `snap` capture again. A page whose main thread is held
//! when the watch is armed is not kept.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_capture_reuse_more`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::Command;
use std::time::Duration;

use common::{Harness, Run, live, serve};
use serde_json::json;
use tabref_cdp::{attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;

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

const BUSY: &str = r#"<!doctype html><html><head><title>Busy</title></head><body>
<h1>Guide</h1><p>Alpha needle text.</p>
<script>addEventListener('load', () => setTimeout(() => { const until = Date.now() + 1500; while (Date.now() < until); }, 0));</script>
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

#[test]
fn a_page_too_busy_to_arm_the_watch_is_not_kept() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-busy");
    let started = std::time::Instant::now();
    let opened = open(&tabref, BUSY);
    assert!(opened.stdout.contains("Alpha needle"), "{}", opened.stdout);
    let found = run(&tabref, &["find", "needle"], &[]);
    assert_ne!(
        snapshot_id(&found),
        snapshot_id(&opened),
        "kept a capture whose watch was armed late"
    );
    assert!(started.elapsed() < Duration::from_secs(8));
    stop(&tabref);
}

fn direct_page_command(tabref: &Harness, method: &str, params: serde_json::Value) {
    let deadline = Deadline::after(5_000).unwrap();
    let mut connection = connect(&tabref.browser.endpoint().unwrap(), deadline).unwrap();
    let pages = list_page_targets(&mut connection, deadline).unwrap();
    let page = pages
        .iter()
        .find(|page| page.url.starts_with("http:"))
        .unwrap();
    let page = attach_to_target(&mut connection, &page.target_id, deadline).unwrap();
    let reply = connection
        .call(method, params, Some(&page.session_id), deadline)
        .unwrap();
    assert!(reply.get("exceptionDetails").is_none(), "{reply}");
}

fn kept_snapshot(tabref: &Harness) -> Run {
    let mut captured = run(tabref, &["snap"], &[]);
    for _ in 0..3 {
        let next = run(tabref, &["snap"], &[]);
        if snapshot_id(&captured) == snapshot_id(&next) {
            return next;
        }
        captured = next;
    }
    panic!("the fixture never reused a capture");
}

#[test]
fn a_read_with_a_slow_watch_check_keeps_the_capture_for_the_next_read() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-slow-check");
    open(&tabref, STILL);
    for _ in 0..5 {
        let kept = kept_snapshot(&tabref);
        direct_page_command(
            &tabref,
            "Runtime.evaluate",
            json!({
                "expression": r#"(() => {
                const check = Element.prototype.checkVisibility;
                Element.prototype.checkVisibility = function(options) {
                    Element.prototype.checkVisibility = check;
                    const until = performance.now() + 60;
                    while (performance.now() < until) {}
                    return check.call(this, options);
                };
            })()"#
            }),
        );
        let read = run(&tabref, &["read", "--section", "More"], &[]);
        assert!(read.stdout.contains("Beta words"), "{}", read.stdout);
        let found = run(&tabref, &["find", "needle"], &[]);
        if snapshot_id(&found) == snapshot_id(&kept) {
            stop(&tabref);
            return;
        }
    }
    stop(&tabref);
    panic!("no read with a timed-out check kept its capture for the following find");
}

#[test]
fn css_hover_and_stylesheet_edits_invalidate_a_kept_view() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-css");
    open(
        &tabref,
        r#"<!doctype html><meta charset=utf-8><style>
body { margin: 0 }
#trigger { display: block; width: 100px; height: 60px }
#trigger:not(:hover) + #menu, #toast { visibility: hidden }
</style><button id=trigger>Menu</button><a id=menu href=#settings>Settings entry</a>
<p id=toast>New notification</p>"#,
    );
    let opened = kept_snapshot(&tabref);
    assert!(
        !opened.stdout.contains("Settings entry"),
        "{}",
        opened.stdout
    );
    direct_page_command(
        &tabref,
        "Input.dispatchMouseEvent",
        json!({"type": "mouseMoved", "x": 20, "y": 20}),
    );
    let revealed = run(&tabref, &["snap"], &[]);
    assert_ne!(snapshot_id(&opened), snapshot_id(&revealed));
    assert!(
        revealed.stdout.contains("Settings entry"),
        "{}",
        revealed.stdout
    );
    let revealed = kept_snapshot(&tabref);
    direct_page_command(
        &tabref,
        "Runtime.evaluate",
        json!({"expression": "document.styleSheets[0].insertRule('#toast { visibility: visible }', 3)"}),
    );
    let toast = run(&tabref, &["snap"], &[]);
    assert_ne!(snapshot_id(&revealed), snapshot_id(&toast));
    assert!(
        toast.stdout.contains("New notification"),
        "{}",
        toast.stdout
    );
    stop(&tabref);
}

#[test]
fn a_late_closed_shadow_root_is_not_replayed() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-late-closed");
    open(
        &tabref,
        "<!doctype html><h1>Open page</h1><div id=host></div>",
    );
    let opened = kept_snapshot(&tabref);
    direct_page_command(
        &tabref,
        "Runtime.evaluate",
        json!({
            "expression": "document.getElementById('host').attachShadow({mode: 'closed'}).innerHTML = '<p>Late closed panel</p>'"
        }),
    );
    let changed = run(&tabref, &["snap"], &[]);
    assert_ne!(snapshot_id(&opened), snapshot_id(&changed));
    assert!(
        changed.stdout.contains("Late closed panel"),
        "{}",
        changed.stdout
    );
    stop(&tabref);
}

#[test]
fn an_oversized_page_is_never_kept() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-oversized");
    let html = format!(
        "<!doctype html><h1>Large page</h1>{}",
        "<span>Row</span>".repeat(1_200)
    );
    let port = serve(&[("/", &html)]);
    let opened = run(
        &tabref,
        &["open", &format!("http://127.0.0.1:{port}/")],
        &[],
    );
    let changed = run(&tabref, &["snap"], &[]);
    assert_ne!(snapshot_id(&opened), snapshot_id(&changed));
    stop(&tabref);
}
