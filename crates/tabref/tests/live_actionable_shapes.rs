//! Live regressions for the shapes the actionability gate must not refuse
//! or slow down: a field, select or key target with no box (a custom
//! dropdown's select, a one-time-code input, an empty inline editor), a
//! `display: contents` control, a toggle switch whose input only its label
//! draws, and boxless links that fail at once with advice instead of after
//! a wait. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_actionable_shapes -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const SHAPES: &str = r##"<!doctype html><title>Shapes</title>
<style>
.zero { opacity: 0; width: 0; height: 0; border: 0; padding: 0; }
.switch { position: relative; display: inline-block; width: 60px; height: 34px; }
.switch input { opacity: 0; width: 0; height: 0; }
.slider { position: absolute; inset: 0; background: #ccc; }
.sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0, 0, 0, 0); }
.float a { display: block; }
.float a span { float: left; }
.card { position: relative; width: 240px; height: 40px; border: 1px solid; }
.card a::after { content: ''; position: absolute; inset: 0; }
</style>
<script>function out(text) { document.getElementById('out').textContent = text; }</script>
<p id=out>Nothing</p>
<label>Hidden size <select class=zero onchange="out('size:' + this.value)"><option>Small</option><option>Medium</option></select></label>
<label>Code digit <input class=zero oninput="out('code:' + this.value)" onkeydown="if (event.key === 'Enter') out('code enter')"></label>
<p>Inline: <span contenteditable=true role=textbox aria-label="Inline note" oninput="out('inline:' + this.textContent)"></span></p>
<button style="display: contents" onclick="out('contents')">Contents button</button>
<x-pill role=button tabindex=0 aria-label="Host pill" onclick="out('pill')"></x-pill>
<label class=switch><input type=checkbox onchange="out('switch:' + this.checked)"><span class=slider></span><span class=sr>Dark mode</span></label>
<div class=card>Product <a href="#" aria-label="Open product" onclick="out('stretched'); return false"></a></div>
<div class=float><a href="#" onclick="out('float'); return false"><span>Floaty</span></a></div>
<script>
customElements.define('x-pill', class extends HTMLElement {
  connectedCallback() {
    if (!this.shadowRoot) {
      this.attachShadow({ mode: 'open' }).innerHTML = '<style>:host { display: contents; }</style><div style="padding: 8px">Host pill</div>';
    }
  }
});
</script>"##;

const ANON: &str = r#"<!doctype html><title>Anon</title><p>Tools</p><button disabled onclick="document.title = 'clicked'"><svg width=16 height=16><rect width=16 height=16></rect></svg></button>"#;

fn site() -> (Harness, String) {
    let port = serve(&[("/shapes", SHAPES), ("/anon", ANON)]);
    let tabref = harness("actionable-shapes");
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&["open", &format!("{base}/shapes")]);
    (tabref, base)
}

fn out(tabref: &Harness) -> String {
    let out = tabref.ok(&["js", "document.getElementById('out').textContent"]);
    out.stdout.trim().to_owned()
}

#[test]
fn a_field_select_or_key_target_with_no_box_still_takes_its_action() {
    if !live() {
        return;
    }
    let (tabref, _) = site();
    tabref.ok(&["select", "Hidden size", "Medium"]);
    assert_eq!(out(&tabref), "size:Medium");
    tabref.ok(&["fill", "Code digit", "7"]);
    assert_eq!(out(&tabref), "code:7");
    tabref.ok(&["press", "Enter", "Code digit"]);
    assert_eq!(out(&tabref), "code enter");
    tabref.ok(&["fill", "Inline note", "hi"]);
    assert_eq!(out(&tabref), "inline:hi");
}

#[test]
fn a_display_contents_control_is_clicked() {
    if !live() {
        return;
    }
    let (tabref, _) = site();
    tabref.ok(&["click", "Contents button"]);
    assert_eq!(out(&tabref), "contents");
    tabref.ok(&["click", "Host pill"]);
    assert_eq!(out(&tabref), "pill");
}

#[test]
fn a_toggle_whose_input_has_no_box_is_set_through_its_label() {
    if !live() {
        return;
    }
    let (tabref, _) = site();
    tabref.ok(&["check", "Dark mode"]);
    assert_eq!(out(&tabref), "switch:true");
}

#[test]
fn a_link_with_no_box_fails_at_once_with_advice() {
    if !live() {
        return;
    }
    let (tabref, _) = site();
    let run = tabref.run(&["--json", "click", "Floaty"]);
    let json = run.json();
    assert_eq!(
        json["error"]["details"]["kind"], "element_zero_size",
        "{}",
        run.stdout
    );
    assert_eq!(json["error"]["details"]["waited_ms"], 0, "{}", run.stdout);
    let message = json["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("has no size on screen; click the text or label that draws it"),
        "{message}"
    );
    let card = tabref.run(&["--json", "click", "Open product"]).json();
    assert_eq!(card["error"]["details"]["kind"], "element_zero_size");
    assert_eq!(card["error"]["details"]["waited_ms"], 0);
    assert_eq!(out(&tabref), "Nothing");
}

#[test]
fn an_unnamed_target_is_named_by_its_role_once() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    let opened = tabref.ok(&["open", &format!("{base}/anon")]);
    let run = tabref.run(&["do", "click button"]);
    assert!(
        run.stderr
            .starts_with("error ACTION_FAILED (element_disabled): step 1 `click button`: button is disabled; wait for the page to enable it, or fill what it needs"),
        "{}\n{}",
        run.stderr,
        opened.stdout
    );
}
