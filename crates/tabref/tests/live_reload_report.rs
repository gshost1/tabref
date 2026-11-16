//! Live run of what an action that reloads the same address prints: the
//! changed lines, then the reloaded page's view with its new refs, as a
//! move to another address does, so a form sent back with errors shows
//! which values it kept without a `snap`.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_reload_report`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const FORM: &str = r#"<!doctype html><title>Book</title><h1>Book</h1>
<p id="err"></p>
<label>Phone <input id="phone"></label>
<label><input type="checkbox" id="ok"> Details are correct</label>
<button type="button" id="save">Book it</button>
<script>
const kept = sessionStorage.getItem('phone');
if (kept !== null) {
  document.getElementById('phone').value = kept;
  document.getElementById('err').textContent = 'Phone must be digits and dashes.';
}
document.getElementById('save').addEventListener('click', () => {
  sessionStorage.setItem('phone', document.getElementById('phone').value);
  location.reload();
});
</script>"#;

#[test]
fn a_reload_of_the_same_address_prints_the_new_view() {
    if !live() {
        return;
    }
    let port = serve(&[("/book", FORM)]);
    let tabref = harness("reload-report");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/book")]);
    let sent = tabref.ok(&[
        "do",
        "fill Phone \"(555) 123\"\ncheck \"Details are correct\"\nclick \"Book it\"",
    ]);
    assert!(
        sent.stdout.contains("+ Phone must be digits and dashes."),
        "{}",
        sent.stdout
    );
    assert!(
        sent.stdout.contains(r#"textbox Phone = "(555) 123""#),
        "the reloaded form is not shown:\n{}",
        sent.stdout
    );
    assert!(
        sent.stdout.contains("checkbox Details are correct]"),
        "{}",
        sent.stdout
    );
    assert!(
        !sent.stdout.contains("tabref snap for new ones"),
        "{}",
        sent.stdout
    );
    let _ = tabref.run(&["daemon", "stop"]);
}
