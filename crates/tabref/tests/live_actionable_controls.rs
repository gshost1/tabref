//! Live regressions for presses that land on something other than their
//! target: a boxless checkbox is never set through a label that holds
//! another control (a help button, a terms link), a `display: contents`
//! control is never pressed through a separate control it draws (a link in
//! its shadow root, a row's checkbox), and an image-map area is refused at
//! once. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_actionable_controls -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const CONTROLS: &str = r##"<!doctype html><title>Controls</title>
<style>.zero { opacity: 0; width: 0; height: 0; margin: 0; padding: 0; border: 0; }</style>
<script>
function out(text) {
  const o = document.getElementById('out');
  o.textContent = (o.textContent === 'Nothing' ? '' : o.textContent + '|') + text;
}
</script>
<p id=out>Nothing</p>
<div><label><input type=checkbox class=zero onchange="out('remember')"><button type=button onclick="out('help')">Remember me help</button></label></div>
<div><label style="display: block; width: 320px"><input type=checkbox class=zero onchange="out('accept')"><a href="#" style="display: block" onclick="out('terms'); return false">Accept the terms link</a></label></div>
<div role=button aria-label="Row go" style="display: contents" onclick="out('row go')"><a href="#" onclick="out('nested link'); return false">Nested link</a><span>after</span></div>
<div style="display: grid; grid-template-columns: 40px 200px"><div role=button aria-label="Row Alice" style="display: contents" onclick="out('row alice')"><span role=cell style="display: block"><input type=checkbox style="width: 40px; height: 20px; margin: 0" aria-label="select Alice" onchange="out('select alice')"></span><span role=cell>Alice</span></div></div>
<x-dc role=button aria-label="Host dc" onclick="out('host dc')"></x-dc>
<x-wrap role=button aria-label="Wrap host" onclick="out('wrap host')"></x-wrap>
<img src="data:image/gif;base64,R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==" width=60 height=30 usemap="#m"><map name=m><area shape=rect coords="0,0,60,30" href="#" alt="Area go" onclick="out('area'); return false"></map>
<script>
customElements.define('x-dc', class extends HTMLElement {
  connectedCallback() {
    if (!this.shadowRoot) {
      this.attachShadow({ mode: 'open' }).innerHTML = '<style>:host { display: contents; }</style><a href="#" onclick="event.preventDefault(); out(\'host link\')">host link</a> <span>Host label</span>';
    }
  }
});
customElements.define('x-wrap', class extends HTMLElement {
  connectedCallback() {
    if (!this.shadowRoot) {
      this.attachShadow({ mode: 'open' }).innerHTML = '<style>:host { display: contents; }</style><button onclick="out(\'wrapped\')">Wrap</button>';
    }
  }
});
</script>"##;

fn site() -> Harness {
    let port = serve(&[("/controls", CONTROLS)]);
    let tabref = harness("actionable-controls");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/controls")]);
    tabref
}

fn out(tabref: &Harness) -> String {
    let out = tabref.ok(&["js", "document.getElementById('out').textContent"]);
    out.stdout.trim().to_owned()
}

fn refused_at_once(tabref: &Harness, args: &[&str], kind: &str) {
    let mut full = vec!["--json"];
    full.extend_from_slice(args);
    let run = tabref.run(&full);
    let json = run.json();
    assert_eq!(json["error"]["details"]["kind"], kind, "{}", run.stdout);
    assert_eq!(json["error"]["details"]["waited_ms"], 0, "{}", run.stdout);
}

#[test]
fn a_boxless_checkbox_is_not_set_through_a_label_that_holds_another_control() {
    if !live() {
        return;
    }
    let tabref = site();
    refused_at_once(&tabref, &["check", "Remember me help"], "element_zero_size");
    refused_at_once(
        &tabref,
        &["check", "Accept the terms link"],
        "element_zero_size",
    );
    assert_eq!(out(&tabref), "Nothing");
}

#[test]
fn a_display_contents_control_is_pressed_beside_the_controls_it_draws() {
    if !live() {
        return;
    }
    let tabref = site();
    tabref.ok(&["click", "Host dc"]);
    assert_eq!(out(&tabref), "host dc");
    tabref.ok(&[
        "js",
        "document.getElementById('out').textContent = 'Nothing'; 1",
    ]);
    tabref.ok(&["click", "Row go"]);
    assert_eq!(out(&tabref), "row go");
    tabref.ok(&[
        "js",
        "document.getElementById('out').textContent = 'Nothing'; 1",
    ]);
    tabref.ok(&["click", "Row Alice"]);
    assert_eq!(out(&tabref), "row alice");
}

#[test]
fn a_display_contents_host_whose_only_control_is_inside_presses_it() {
    if !live() {
        return;
    }
    let tabref = site();
    tabref.ok(&["click", "Wrap host"]);
    assert_eq!(out(&tabref), "wrapped|wrap host");
}

#[test]
fn an_image_map_area_is_refused_at_once() {
    if !live() {
        return;
    }
    let tabref = site();
    refused_at_once(&tabref, &["click", "Area go"], "element_not_pressable");
    assert_eq!(out(&tabref), "Nothing");
}
