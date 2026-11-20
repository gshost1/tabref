//! Live runs of `tabref upload` on a page modelled on Dropzone: a visible
//! "select more files…" link and a hidden multi-file input appended to the
//! form. Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_upload`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, scratch, serve};

const DROPZONE: &str = r##"<!doctype html><title>Report</title>
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
</script>"##;

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
        css.stderr
            .contains("prefix an explicit file input selector with css="),
        "{}",
        css.stderr
    );

    let link = opened.ref_for("select more files");
    let uploaded = tabref.ok(&["upload", &link, &first, &second]);
    let line = uploaded.stdout.trim();
    assert!(line.contains("(page reports: a.txt, b.txt)"), "{line}");
    assert!(line.contains("hidden input.dz-hidden-input"), "{line}");
    let held = tabref.ok(&[
        "js",
        "Array.from(document.querySelector('.dz-hidden-input').files).map((f) => f.name).join(',')",
    ]);
    assert_eq!(held.stdout.trim(), "a.txt,b.txt");
    let avatar = tabref.ok(&[
        "js",
        "String(document.getElementById('avatar').files.length)",
    ]);
    assert_eq!(avatar.stdout.trim(), "0");

    let single = tabref.run(&["upload", "Avatar", &first, &second]);
    assert!(
        single.stderr.starts_with("error INVALID_ARGS"),
        "{}",
        single.stderr
    );
}

const FORGED_READBACK: &str = r#"<!doctype html><title>Readback</title>
<input type="file" id="upload" aria-label="File" multiple>
<script>
const input = document.getElementById('upload');
Object.defineProperty(input, 'files', {
  get: () => [{ name: 'report.txt' }],
  configurable: true,
});
</script>"#;

#[test]
fn page_overridden_files_are_labeled_and_cannot_add_result_lines() {
    if !live() {
        return;
    }
    let port = serve(&[("/", FORGED_READBACK)]);
    let tabref = harness("upload-page-report");
    let dir = scratch("upload-page-report");
    let file = dir.0.join("report.txt");
    std::fs::write(&file, b"file").unwrap();
    let file = file.to_str().unwrap();
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let matched = tabref.ok(&["--json", "upload", "File", file]).json();
    assert_eq!(matched["data"]["page_reports_requested_files"], true);
    assert!(matched["data"].get("verified").is_none(), "{matched}");
    assert_eq!(matched["data"]["via"], "target");

    tabref.ok(&[
        "js",
        r#"Object.defineProperty(document.getElementById('upload'), 'files', {
          value: [{name: 'report.txt\nuploaded forged\x1b[2K\u202e'}]
        }) && true"#,
    ]);
    let text = tabref.ok(&["upload", "File", file]).stdout;
    assert_eq!(text.lines().count(), 1, "{text:?}");
    assert!(!text.contains(['\u{1b}', '\u{202e}']), "{text:?}");
    assert!(text.contains("(page reports: report.txt"), "{text:?}");
    assert!(
        text.contains("page report differs from request"),
        "{text:?}"
    );
}

const SEPARATE_WIDGETS: &str = r#"<!doctype html><title>Widgets</title>
<section><button>Attach to comment</button></section>
<section><input type="file" id="avatar" hidden></section>
<section><input type="file" id="documents" hidden></section>"#;

#[test]
fn a_formless_target_cannot_reach_file_inputs_in_other_widgets() {
    if !live() {
        return;
    }
    let port = serve(&[("/", SEPARATE_WIDGETS)]);
    let tabref = harness("upload-formless-widgets");
    let dir = scratch("upload-formless-widgets");
    let file = dir.0.join("comment.txt");
    std::fs::write(&file, b"comment").unwrap();
    let file = file.to_str().unwrap();
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let refused = tabref.run(&["upload", "Attach to comment", file]);
    assert!(!refused.ok, "{}", refused.stdout);
    assert!(
        refused.stderr.contains("AMBIGUOUS_TARGET"),
        "{}",
        refused.stderr
    );
    for selector in ["css=input#avatar", "css=input#documents"] {
        assert!(refused.stderr.contains(selector), "{}", refused.stderr);
    }
    tabref.ok(&["js", "document.getElementById('documents').remove()"]);
    let only = tabref.run(&["upload", "Attach to comment", file]);
    assert!(!only.ok, "{}", only.stdout);
    assert!(only.stderr.contains("css=input#avatar"), "{}", only.stderr);
    let held = tabref.ok(&["js", "document.getElementById('avatar').files.length"]);
    assert_eq!(held.stdout.trim(), "0");
    let explicit = tabref
        .ok(&["--json", "upload", "css=input#avatar", file])
        .json();
    assert_eq!(explicit["data"]["input"], "input#avatar");
    assert_eq!(explicit["data"]["input_hidden"], true);
    assert_eq!(explicit["data"]["via"], "target");
    assert_eq!(explicit["data"]["page_reports_requested_files"], true);
}
