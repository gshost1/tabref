//! Live runs of `tabref upload` on a page modelled on Dropzone: a visible
//! "select more files…" link and a hidden multi-file input appended to the
//! form. Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_upload`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, scratch, serve};

const DROPZONE: &str = r#"<!doctype html><title>Report</title>
<form id="report"><label>Description <textarea name="body"></textarea></label>
<div class="dropzone">Drop files here or <a href="#" id="pick">select more files…</a></div>
<button>Submit report</button></form>
<form id="profile"><label>Avatar <input type="file" id="avatar" name="avatar"></label></form>
<script>
const input = document.createElement('input');
input.type = 'file';
input.multiple = true;
input.className = 'dz-hidden-input';
input.style.cssText = 'visibility:hidden;position:absolute;top:0;left:0;height:0;width:0';
document.getElementById('report').appendChild(input);
</script>"#;

#[test]
fn a_drop_zone_link_uploads_to_its_hidden_input_and_css_is_explained() {
    if !live() {
        return;
    }
    let port = serve(&[("/", DROPZONE)]);
    let tabref = harness("upload");
    let files = scratch("upload");
    let first = files.0.join("a.txt");
    let second = files.0.join("b.txt");
    std::fs::write(&first, b"a").unwrap();
    std::fs::write(&second, b"b").unwrap();
    let first = first.to_string_lossy().into_owned();
    let second = second.to_string_lossy().into_owned();
    let opened = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);

    let css = tabref.run(&["upload", "input[type=file]", &first, &second]);
    assert!(!css.ok, "{}", css.stdout);
    assert!(
        css.stderr.contains("refs or visible names"),
        "{}",
        css.stderr
    );

    let link = opened.ref_for("select more files");
    let uploaded = tabref.ok(&["upload", &link, &first, &second]);
    let line = uploaded.stdout.trim();
    assert!(line.starts_with("uploaded a.txt, b.txt"), "{line}");
    assert!(line.contains("hidden input.dz-hidden-input"), "{line}");
    let held = tabref.ok(&[
        "js",
        "Array.from(document.querySelector('.dz-hidden-input').files).map((f) => f.name).join(',')",
    ]);
    assert_eq!(held.stdout.trim(), "a.txt,b.txt");
    let avatar = tabref.ok(&["js", "String(document.getElementById('avatar').files.length)"]);
    assert_eq!(avatar.stdout.trim(), "0");

    let single = tabref.run(&["upload", "Avatar", &first, &second]);
    assert!(
        single.stderr.starts_with("error INVALID_ARGS"),
        "{}",
        single.stderr
    );
}
