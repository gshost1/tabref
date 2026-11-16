//! The page the `focus` tests share, and the ways to open it: in a browser
//! tabref treats as one it launched, or in one it did not.

use super::{Harness, harness, serve};

pub const PAGE: &str = r#"<!doctype html><title>Focus</title>
<label>Name <input id=name onfocus="document.getElementById('out').textContent = 'name focused'"></label>
<label>City <input id=city></label>
<label>Locked <input id=locked disabled></label>
<div id=tap onclick="document.title = 'tapped'">Tap area</div>
<a id=bare onclick="document.title = 'bare'">Bare link</a>
<label id=boxlabel onclick="1">Hidden box <input id=box type=checkbox style="display:none"></label>
<label id=ziplabel for=zip onclick="1">Zip code</label> <input id=zip aria-label="Postal">
<label>Jumpy <input id=jumpy onfocus="setTimeout(() => document.getElementById('city').focus(), 50)"></label>
<label>Late jump <input id=late onfocus="setTimeout(() => document.getElementById('city').focus(), 220)"></label>
<label>Slow <input id=slow onfocus="const t = Date.now(); while (Date.now() - t < 2500) {}"></label>
<label>Search <input id=search
  onfocus="document.getElementById('list').hidden = false"
  onblur="document.getElementById('list').hidden = true"></label>
<ul id=list hidden><li>Suggestion one</li></ul>
<div id=badindex tabindex="abc" onclick="1">Bad index</div>
<svg width=120 height=30><a id=svglink xlink:href=#svg><text x=5 y=20>Svg link</text></a></svg>
<x-field id=xfield role=button aria-label="Delegated"></x-field>
<x-outer id=outer role=button aria-label="Nested"></x-outer>
<x-skip id=skip role=button aria-label="Skips hidden"></x-skip>
<x-empty id=empty role=button aria-label="Empty host"></x-empty>
<p id=out>Nothing</p>
<p id=seen>Unseen</p>
<script>
const host = html => class extends HTMLElement {
  constructor() { super(); this.attachShadow({ mode: 'open', delegatesFocus: true }).innerHTML = html; }
};
customElements.define('x-field', host('<input id=inner>'));
customElements.define('x-outer', host('<x-field id=middle></x-field>'));
customElements.define('x-skip', host('<input type=hidden id=secret><input id=unseen style="display:none"><span id=frozen contenteditable=false>Frozen</span><input id=real>'));
customElements.define('x-empty', host('<span>Nothing to focus</span>'));
window.ticks = { hidden: 0, wentHidden: false };
setInterval(() => { if (document.visibilityState === 'hidden') window.ticks.hidden += 1; }, 10);
document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'hidden') window.ticks.wentHidden = true; });
document.addEventListener('visibilitychange', () => { document.getElementById('seen').textContent = 'vis changed'; });
window.addEventListener('focus', () => { document.getElementById('seen').textContent = 'window focused'; });
</script>"#;

/// A page with no focus listener on the window or the document: one field
/// with none at all, and one whose ancestor moves the focus on `focusin`.
pub const QUIET: &str = r#"<!doctype html><title>Quiet</title>
<label>Plain <input id=plain></label>
<label>Other <input id=other></label>
<div onfocusin="if (event.target.id === 'deep') setTimeout(() => document.getElementById('other').focus(), 220)">
<label>Deep jump <input id=deep></label></div>"#;

/// The focus page in a browser tabref did not launch (`--cdp`).
pub fn foreign_site(name: &str) -> Harness {
    let tabref = harness(name);
    open_page(&tabref);
    tabref
}

/// The focus page in a browser tabref treats as one `open --launch`
/// started: its launch record names the harness browser.
pub fn own_site(name: &str) -> Harness {
    let tabref = own_browser(name);
    open_page(&tabref);
    tabref
}

/// A harness browser tabref treats as one `open --launch` started.
pub fn own_browser(name: &str) -> Harness {
    let tabref = harness(name);
    let record = serde_json::to_string(&tabref.browser).unwrap();
    std::fs::write(tabref.home.join("launched.json"), record).unwrap();
    tabref
}

/// Opens the focus page (`/focus`, with `/quiet` beside it).
pub fn open_page(tabref: &Harness) -> u16 {
    let port = serve(&[("/focus", PAGE), ("/quiet", QUIET)]);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/focus")]);
    port
}

/// The page in tab 0, behind a blank tab that has the front, with what the
/// page saw so far cleared.
pub fn to_back(tabref: &Harness) {
    tabref.ok(&["open", "--new-tab", "about:blank"]);
    in_back(
        tabref,
        "document.getElementById('seen').textContent = 'Unseen'",
    );
}

pub fn in_back(tabref: &Harness, expression: &str) -> String {
    tabref
        .ok(&["--tab", "0", "js", expression])
        .stdout
        .trim()
        .to_owned()
}

pub fn read(tabref: &Harness, expression: &str) -> String {
    tabref.ok(&["js", expression]).stdout.trim().to_owned()
}

pub fn active(tabref: &Harness) -> String {
    read(tabref, "document.activeElement.id")
}
