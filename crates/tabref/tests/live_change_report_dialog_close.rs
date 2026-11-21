#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};

const MODAL: &str = r#"<!doctype html><title>Tokens</title>
<p>This cannot be undone. Type X to confirm.</p><p id=notice>Outside notice</p>
<table><tr><td id=row>TRK-100 active</td></tr></table><p id=toast></p>
<dialog aria-labelledby=title><h2 id=title>Revoke token?</h2>
<p>This cannot be undone. Type X to confirm.</p><label>Confirmation <input></label>
<div><button onclick="document.querySelector('dialog').close();document.getElementById('toast').textContent='Request cancelled';document.getElementById('row').textContent='TRK-100 retained';document.getElementById('notice').remove()">Cancel</button> <button>Revoke token</button></div></dialog>
<script>document.querySelector('dialog').showModal()</script>"#;

#[test]
fn native_modal_close_keeps_toast_row_and_outside_removal() {
    if !live() {
        return;
    }
    let port = serve(&[("/", MODAL)]);
    let tabref = harness("dialog-close-native");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Cancel"]).stdout;
    println!(
        "=== DIALOG CLOSE: {} bytes ===\n{text}=== END ===",
        text.len()
    );
    assert!(text.contains("- dialog \"Revoke token?\" closed"), "{text}");
    for line in ["Request cancelled", "retained", "- Outside notice"] {
        assert!(text.contains(line), "{text}");
    }
    for line in [
        "- Revoke token?",
        "- This cannot be undone.",
        "- Cancel Revoke token",
        "- Confirmation",
    ] {
        assert!(!text.contains(line), "{text}");
    }
}

#[test]
fn aria_dialogs_in_shadow_roots_and_frames_use_labels_or_headings() {
    if !live() {
        return;
    }
    let inner = r#"<!doctype html><p id=status>Outside ready</p><div role=alertdialog aria-modal=true aria-label="Confirm deletion"><p>Remove the record?</p><button onclick="this.parentElement.hidden=true;document.getElementById('status').textContent='Outside saved'">Dismiss</button></div>"#;
    let shadow = r#"<!doctype html><x-modal></x-modal><script>customElements.define('x-modal',class extends HTMLElement {connectedCallback(){this.attachShadow({mode:'open'}).innerHTML='<div role=dialog aria-modal=true><h2>Shadow confirm</h2><p>Delete this record?</p><button>Dismiss</button></div><p>Outside shadow</p>';this.shadowRoot.querySelector('button').onclick=()=>this.shadowRoot.querySelector('div').remove();}})</script>"#;
    let port = serve(&[
        ("/inner", inner),
        ("/frame", "<!doctype html><iframe src=/inner></iframe>"),
        ("/shadow", shadow),
    ]);
    let tabref = harness("dialog-close-roots");
    for (path, expected) in [
        ("frame", "- alertdialog \"Confirm deletion\" closed"),
        ("shadow", "- dialog \"Shadow confirm\" closed"),
    ] {
        tabref.ok(&["open", &format!("http://127.0.0.1:{port}/{path}")]);
        let text = tabref.ok(&["click", "Dismiss"]).stdout;
        assert!(text.contains(expected), "{text}");
        if path == "frame" {
            assert!(text.contains("saved"), "{text}");
        }
    }
}

#[test]
fn remounted_reset_or_reopened_dialogs_keep_their_new_text() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><div role=dialog aria-modal=true aria-label=Confirm><p id=message>Old request</p>
<button onclick="const dialog=this.parentElement;dialog.replaceWith(dialog.cloneNode(true))">Remount</button>
<button onclick="delete window[Symbol.for('tabref.watch')];const dialog=this.parentElement;dialog.replaceWith(dialog.cloneNode(true))">Reset watch</button>
<button onclick="const dialog=this.parentElement;dialog.hidden=true;document.getElementById('message').textContent='New request';dialog.hidden=false">Reopen</button></div>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("dialog-close-remount");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    for label in ["Remount", "Reset watch", "Reopen"] {
        let text = tabref.ok(&["click", label]).stdout;
        assert!(!text.contains("closed"), "{text}");
        if label == "Reopen" {
            assert!(text.contains("{Old -> New} request"), "{text}");
        }
    }
}

#[test]
fn unnamed_native_open_dialog_is_not_confused_with_another_dialog() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><dialog open style="position:static"><p>Static dialog</p></dialog><dialog open style="position:static"><p>Temporary prompt</p><button onclick="this.parentElement.close()">Close prompt</button></dialog>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("dialog-close-unnamed");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Close prompt"]).stdout;
    assert!(text.contains("- dialog closed"), "{text}");
    assert!(!text.contains("- Temporary prompt"), "{text}");
}
