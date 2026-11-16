//! `--dialog accept|dismiss|accept=TEXT` answers the first JavaScript
//! dialog a command meets, and every later one is accepted as before: a
//! confirm dismissed, a prompt answered with text, the second dialog of one
//! click accepted, a confirm in a same-origin and in a cross-origin iframe,
//! and a `beforeunload` on navigation both kept (dismissed) and left
//! (accepted). Each result says which dialog opened and how it was
//! answered. Through the daemon, so the flag reaches it. Live
//! (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const MAIN: &str = "<!doctype html><title>Start</title>\
    <script>onmessage = e => { document.title = e.data; };</script>\
    <button onclick=\"document.title = confirm('Sure?') ? 'yes' : 'no'\">Confirm</button>\
    <button onclick=\"document.title = 'got ' + prompt('Name?', 'def')\">Prompt</button>\
    <button onclick=\"document.title = [confirm('First?'), confirm('Second?')].join()\">Twice</button>\
    <button onclick=\"onbeforeunload = e => { e.preventDefault(); e.returnValue = ''; }; \
    document.title = 'armed'\">Arm</button>\
    <iframe srcdoc=\"<button onclick=&quot;parent.document.title = confirm('Inner?') ? 'iy' : \
    'in'&quot;>Inner</button>\"></iframe>\
    <iframe src='http://localhost:PORT/cross'></iframe>";
const CROSS: &str = "<!doctype html><title>Cross</title>\
    <button onclick=\"parent.postMessage(confirm('Cross?') ? 'cy' : 'cn', '*')\">Cross</button>";
const ELSEWHERE: &str = "<!doctype html><title>Elsewhere</title><p>away";

#[test]
fn dialog_answers_are_chosen_for_the_next_dialog_and_reported() {
    if !live() {
        return;
    }
    let tabref = harness("dialogs");
    let cross_port = serve(&[("/cross", CROSS)]);
    let main = MAIN.replace("PORT", &cross_port.to_string());
    let port = serve(&[("/", &main), ("/away", ELSEWHERE)]);
    let start = format!("http://127.0.0.1:{port}/");
    let away = format!("http://127.0.0.1:{port}/away");
    tabref.ok(&["open", &start]);
    let title = || {
        tabref
            .ok(&["js", "document.title"])
            .stdout
            .trim()
            .to_owned()
    };

    let dismissed = tabref.ok(&["--dialog", "dismiss", "click", "Confirm"]);
    let after_dismiss = title();
    let accepted = tabref.ok(&["click", "Confirm"]);
    let after_accept = title();
    let prompted = tabref.ok(&["--dialog", "accept=Bob", "click", "Prompt"]);
    let after_prompt = title();
    let twice = tabref.ok(&["--dialog", "dismiss", "click", "Twice"]);
    let after_twice = title();
    let inner = tabref.ok(&["--dialog", "dismiss", "click", "Inner"]);
    let after_inner = title();
    let cross = tabref.ok(&["--dialog", "dismiss", "click", "Cross"]);
    let after_cross = title();
    let unused = tabref.ok(&["--dialog", "dismiss", "js", "1 + 1"]);

    tabref.ok(&["click", "Arm"]);
    let kept = tabref.run(&["--dialog", "dismiss", "open", &away]);
    let kept_title = title();
    let left = tabref.ok(&["open", &away]);
    let left_title = title();
    let _ = tabref.run(&["daemon", "stop"]);

    assert!(
        dismissed
            .stdout
            .contains("dialog confirm \"Sure?\" dismissed"),
        "{}",
        dismissed.stdout
    );
    assert_eq!(after_dismiss, "no");
    assert!(
        accepted
            .stdout
            .contains("dialog confirm \"Sure?\" accepted"),
        "{}",
        accepted.stdout
    );
    assert_eq!(after_accept, "yes");
    assert!(
        prompted
            .stdout
            .contains("dialog prompt \"Name?\" answered \"Bob\""),
        "{}",
        prompted.stdout
    );
    assert_eq!(after_prompt, "got Bob");
    assert!(
        twice.stdout.contains("dialog confirm \"First?\" dismissed"),
        "{}",
        twice.stdout
    );
    assert!(
        twice.stdout.contains("dialog confirm \"Second?\" accepted"),
        "{}",
        twice.stdout
    );
    assert_eq!(after_twice, "false,true");
    assert!(
        inner.stdout.contains("dialog confirm \"Inner?\" dismissed"),
        "{}",
        inner.stdout
    );
    assert_eq!(after_inner, "in");
    assert!(
        cross.stdout.contains("dialog confirm \"Cross?\" dismissed"),
        "{}",
        cross.stdout
    );
    assert_eq!(after_cross, "cn");
    assert!(
        unused.stdout.contains("--dialog dismiss was not used"),
        "{}",
        unused.stdout
    );
    assert!(
        kept.stdout.contains("dialog beforeunload") && kept.stdout.contains("dismissed"),
        "{}{}",
        kept.stdout,
        kept.stderr
    );
    assert_eq!(
        kept_title, "armed",
        "a dismissed beforeunload still left the page"
    );
    assert!(
        left.stdout.contains("dialog beforeunload") && left.stdout.contains("accepted"),
        "{}",
        left.stdout
    );
    assert_eq!(left_title, "Elsewhere");
}
