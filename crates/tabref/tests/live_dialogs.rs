//! `--dialog accept|dismiss|accept=TEXT` answers the first confirm, prompt
//! or `beforeunload` a command meets (an alert never takes it), and every
//! later one is accepted as before: a confirm dismissed, also after an
//! alert, a prompt answered with text, the second dialog of one click
//! accepted, a confirm in a same-origin and in a cross-origin iframe, one
//! that `js` raised, and a `beforeunload` on navigation kept (dismissed, by
//! `open` and by `do`'s open step) and left (accepted). Each result says
//! which dialog opened and how it was answered. Through the daemon, so the
//! flag reaches it. Live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const MAIN: &str = "<!doctype html><title>Start</title>\
    <script>onmessage = e => { document.title = e.data; };</script>\
    <button onclick=\"document.title = confirm('Sure?') ? 'yes' : 'no'\">Confirm</button>\
    <button onclick=\"document.title = 'got ' + prompt('Name?', 'def')\">Prompt</button>\
    <button onclick=\"document.title = [confirm('First?'), confirm('Second?')].join()\">Twice</button>\
    <button onclick=\"alert('Heads up'); document.title = confirm('Delete all?') ? 'deleted' : 'kept'\">\
    Warn</button>\
    <button onclick=\"setTimeout(() => { document.title = confirm('Later?') ? 'ly' : 'ln'; }, 300)\">\
    Later</button>\
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
    let warned = tabref.ok(&["--dialog", "dismiss", "click", "Warn"]);
    let after_warn = title();
    let asked = tabref.ok(&["--dialog", "dismiss", "js", "confirm('Js?') ? 'y' : 'n'"]);
    let later = tabref.ok(&["--dialog", "dismiss", "click", "Later"]);
    std::thread::sleep(std::time::Duration::from_millis(800));
    let after_later = title();

    let do_open = format!("click Arm; open {away}");
    let batch = tabref.run(&["--dialog", "dismiss", "do", &do_open]);
    let batch_title = title();
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
        warned.stdout.contains("dialog alert \"Heads up\" accepted"),
        "{}",
        warned.stdout
    );
    assert!(
        warned
            .stdout
            .contains("dialog confirm \"Delete all?\" dismissed"),
        "the alert took the choice: {}",
        warned.stdout
    );
    assert_eq!(after_warn, "kept");
    assert!(asked.stdout.contains('n'), "{}", asked.stdout);
    assert!(
        asked.stdout.contains("dialog confirm \"Js?\" dismissed"),
        "a dialog js raised was not reported: {}",
        asked.stdout
    );
    assert!(
        later.stdout.contains("one that opens after it is accepted"),
        "{}",
        later.stdout
    );
    assert_eq!(
        after_later, "ly\ndialog confirm \"Later?\" accepted",
        "the next command without --dialog accepts the late dialog and reports it"
    );
    assert!(batch.ok, "{}{}", batch.stdout, batch.stderr);
    assert!(
        batch.stdout.contains("stayed on the page"),
        "{}",
        batch.stdout
    );
    assert_eq!(batch_title, "armed", "do's open step left the page");
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

const LATER: &str = "<!doctype html><title>Later</title>\
    <button onclick=\"setTimeout(() => { document.title = String(confirm('Later?')); }, 300)\">\
    Later</button>";

/// A dialog that opens after its command ended (here from a timer) is
/// answered when the next command starts, under that command's `--dialog`,
/// and reported by it; a next command without `--dialog` accepts it and
/// reports that.
#[test]
fn a_dialog_that_opens_after_a_command_is_answered_and_reported_by_the_next() {
    if !live() {
        return;
    }
    let tabref = harness("dialogs-later");
    let port = serve(&[("/", LATER)]);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);

    let first = tabref.ok(&["--dialog", "dismiss", "click", "Later"]);
    std::thread::sleep(std::time::Duration::from_millis(800));
    let next = tabref.ok(&["--dialog", "dismiss", "js", "1"]);
    let dismissed_title = tabref
        .ok(&["js", "document.title"])
        .stdout
        .trim()
        .to_owned();

    tabref.ok(&["click", "Later"]);
    std::thread::sleep(std::time::Duration::from_millis(800));
    let plain = tabref.ok(&["js", "1"]);
    let accepted_title = tabref
        .ok(&["js", "document.title"])
        .stdout
        .trim()
        .to_owned();
    let _ = tabref.run(&["daemon", "stop"]);

    assert!(first.stdout.contains("was not used"), "{}", first.stdout);
    assert!(
        next.stdout.contains("dialog confirm \"Later?\" dismissed"),
        "the next command did not answer and report the late dialog: {}",
        next.stdout
    );
    assert!(!next.stdout.contains("was not used"), "{}", next.stdout);
    assert_eq!(dismissed_title, "false");
    assert!(
        plain.stdout.contains("dialog confirm \"Later?\" accepted"),
        "{}",
        plain.stdout
    );
    assert_eq!(accepted_title, "true");
}
