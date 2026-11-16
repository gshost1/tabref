//! Live runs of the change check's edges inside shadow roots and frames: a
//! shadow root the action itself attaches, a large page with a frame, page
//! text with terminal control characters, and cross-site frames that are
//! off screen or tracking pixels. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_act_frames_edges`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const NOTHING: &str = "(no visible change)";

const LATE_SHADOW: &str = r#"<!doctype html><title>Late</title><h1>Orders</h1><div id=late></div>
<button onclick="document.getElementById('late').attachShadow({ mode: 'open' }).innerHTML = '<p>Late panel opened</p>'">Open late</button>"#;

const CONTROL: &str = r#"<!doctype html><title>Alerts</title><p id=out>Alert idle</p>
<button onclick="document.getElementById('out').textContent = 'Alert \u001b[2K\u001b[31mred\u0007 done'">Raise alert</button>"#;

#[test]
fn a_shadow_root_the_click_attaches_is_read_and_its_change_printed_once() {
    if !live() {
        return;
    }
    let port = serve(&[("/late", LATE_SHADOW)]);
    let tabref = harness("frames-late");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/late")]);
    let opened = tabref.ok(&["click", "Open late"]);
    assert!(!opened.stdout.contains(NOTHING), "{}", opened.stdout);
    assert_eq!(
        opened.stdout.matches("Late panel opened").count(),
        1,
        "{}",
        opened.stdout
    );
}

#[test]
fn a_frame_never_costs_the_main_documents_diff_on_a_large_page() {
    if !live() {
        return;
    }
    let lines: String = (0..6_400)
        .map(|at| format!("Ledger row {at:05} settled\n"))
        .collect();
    let page = format!(
        r#"<!doctype html><title>Ledger</title><p id=state>Status idle</p>
<button onclick="document.getElementById('state').textContent = 'Status reconciled'">Reconcile</button>
<iframe src="/notes" title="Notes" style="width:600px;height:200px"></iframe><pre>{lines}</pre>"#
    );
    let notes: String = (0..2_400)
        .map(|at| format!("Note line {at:05} kept\n"))
        .collect();
    let frame = format!("<!doctype html><title>Notes</title><pre>{notes}</pre>");
    assert!(lines.len() > 150_000 && notes.len() > 50_000);
    let port = serve(&[("/ledger", page.as_str()), ("/notes", frame.as_str())]);
    let tabref = harness("frames-large");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/ledger")]);
    let done = tabref.ok(&["click", "Reconcile"]);
    assert!(done.stdout.contains("reconciled"), "{}", done.stdout);
}

#[test]
fn control_characters_in_page_text_are_escaped_not_printed() {
    if !live() {
        return;
    }
    let port = serve(&[("/alerts", CONTROL)]);
    let tabref = harness("frames-control");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/alerts")]);
    let raised = tabref.ok(&["click", "Raise alert"]);
    assert!(!raised.stdout.contains('\u{1b}'), "{:?}", raised.stdout);
    assert!(!raised.stdout.contains('\u{7}'), "{:?}", raised.stdout);
    assert!(raised.stdout.contains("\\u001b"), "{:?}", raised.stdout);
    assert!(raised.stdout.contains("done"), "{:?}", raised.stdout);
}

#[test]
fn an_off_screen_or_pixel_sized_cross_site_frame_does_not_change_the_note() {
    if !live() {
        return;
    }
    let remote = serve(&[("/ad", "<!doctype html><title>Ad</title><p>Buy now</p>")]);
    let page = format!(
        r#"<!doctype html><title>Article</title><h1>Article</h1><button>Noop</button>
<iframe src="http://localhost:{remote}/ad" title="Pixel" style="width:1px;height:1px;border:0"></iframe>
<iframe src="http://localhost:{remote}/ad" title="Beacon" style="width:2px;height:2px;border:0"></iframe>
<div style="height:4000px"></div>
<iframe src="http://localhost:{remote}/ad" title="Footer ad" style="width:300px;height:250px"></iframe>"#
    );
    let port = serve(&[("/article", page.as_str())]);
    let tabref = harness("frames-offscreen");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/article")]);
    let noop = tabref.ok(&["click", "Noop"]);
    assert_eq!(noop.stdout.trim(), format!("clicked \"Noop\" {NOTHING}"));
}
