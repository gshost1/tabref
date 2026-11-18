//! Live checks that `upload_files` finds the file input a drop zone, a label
//! or a form hides behind its visible control, and refuses when it cannot
//! tell which input is meant. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref-page --test live_upload`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod upload_common;

use tabref_core::ErrorCode;
use tabref_page::upload_files;

use common::{connect_to, deadline, launch, live, open_tab, serve};
use upload_common::{files_of, fixture, name_of, text_of, token_of};

const DROPZONE: &str = r##"<!doctype html><html><head><title>Report</title></head><body>
<form id="report" action="/r">
<label>Description <textarea name="body"></textarea></label>
<div class="dropzone"><span>Drop files here or</span> <a href="#" id="pick">select more files…</a></div>
<button>Submit report</button>
</form>
<form id="profile"><label>Avatar <input type="file" id="avatar" name="avatar"></label>
<button id="save">Save profile</button></form>
<div id="cv-box"><label for="cv" id="cv-label">Attach CV</label></div>
<input type="file" id="cv" name="cv" style="display:none">
<h2 id="loose">Elsewhere</h2>
<script>
const input = document.createElement('input');
input.type = 'file';
input.multiple = true;
input.className = 'dz-hidden-input';
input.style.cssText = 'visibility:hidden;position:absolute;top:0;left:0;height:0;width:0';
document.getElementById('report').appendChild(input);
</script>
</body></html>"##;

const NONE: &str = r##"<!doctype html><html><head><title>None</title></head><body>
<a href="#" id="pick">select files…</a>
<button id="attach">Attach files</button><p id="got"></p>
<script>
document.getElementById('attach').onclick = () => {
  const input = document.createElement('input');
  input.type = 'file';
  input.multiple = true;
  input.onchange = () => {
    document.getElementById('got').textContent = Array.from(input.files).map((f) => f.name).join(',');
  };
  input.click();
};
</script></body></html>"##;

const ONLY: &str = r##"<!doctype html><html><head><title>Only</title></head><body>
<a href="#" id="pick">select files…</a>
<script>
const input = document.createElement('input');
input.type = 'file';
input.multiple = true;
input.className = 'dz-hidden-input';
input.style.display = 'none';
document.body.appendChild(input);
</script></body></html>"##;

#[test]
fn a_drop_zone_link_label_and_form_each_reach_their_own_file_input() {
    if !live() {
        return;
    }
    let port = serve(&[("/", DROPZONE)]);
    let chrome = launch();
    let mut connection = connect_to(&chrome);
    let page = open_tab(&mut connection, &format!("http://127.0.0.1:{port}/"));
    let first = fixture("a.txt");
    let second = fixture("b.txt");
    let both = [first.clone(), second.clone()];

    let link = token_of(&mut connection, &page, "#pick");
    let dropped = upload_files(&mut connection, &page, &link, &both, deadline()).unwrap();
    assert!(dropped.verified, "{dropped:?}");
    assert_eq!(
        files_of(&mut connection, &page, ".dz-hidden-input"),
        vec![name_of(&first), name_of(&second)]
    );
    assert!(files_of(&mut connection, &page, "#avatar").is_empty());

    let save = token_of(&mut connection, &page, "#save");
    let saved = upload_files(&mut connection, &page, &save, &both[..1], deadline()).unwrap();
    assert!(saved.verified, "{saved:?}");
    assert_eq!(
        files_of(&mut connection, &page, "#avatar"),
        vec![name_of(&first)]
    );

    let label = token_of(&mut connection, &page, "#cv-label");
    upload_files(&mut connection, &page, &label, &both[1..], deadline()).unwrap();
    assert_eq!(
        files_of(&mut connection, &page, "#cv"),
        vec![name_of(&second)]
    );

    let avatar_label = token_of(&mut connection, &page, "#profile label");
    let refused =
        upload_files(&mut connection, &page, &avatar_label, &both, deadline()).unwrap_err();
    assert_eq!(refused.code, ErrorCode::InvalidArgs, "{refused:?}");
    assert!(refused.message.contains("one file"), "{}", refused.message);
    assert_eq!(
        files_of(&mut connection, &page, "#avatar"),
        vec![name_of(&first)]
    );

    let loose = token_of(&mut connection, &page, "#loose");
    let ambiguous = upload_files(&mut connection, &page, &loose, &both, deadline()).unwrap_err();
    assert_eq!(
        ambiguous.kind(),
        Some("ambiguous_file_input"),
        "{ambiguous:?}"
    );
    assert!(
        ambiguous.message.contains("dz-hidden-input"),
        "{}",
        ambiguous.message
    );
    assert!(
        ambiguous.message.contains("#avatar"),
        "{}",
        ambiguous.message
    );

    std::fs::remove_file(&first).unwrap();
    std::fs::remove_file(&second).unwrap();
}

#[test]
fn the_only_file_input_is_used_and_a_page_without_one_uploads_through_its_chooser() {
    if !live() {
        return;
    }
    let port = serve(&[("/only", ONLY), ("/none", NONE)]);
    let chrome = launch();
    let mut connection = connect_to(&chrome);
    let file = fixture("only.txt");

    let page = open_tab(&mut connection, &format!("http://127.0.0.1:{port}/only"));
    let link = token_of(&mut connection, &page, "#pick");
    let uploaded = upload_files(
        &mut connection,
        &page,
        &link,
        std::slice::from_ref(&file),
        deadline(),
    )
    .unwrap();
    assert!(uploaded.verified, "{uploaded:?}");
    assert_eq!(
        files_of(&mut connection, &page, ".dz-hidden-input"),
        vec![name_of(&file)]
    );

    let empty = open_tab(&mut connection, &format!("http://127.0.0.1:{port}/none"));
    let link = token_of(&mut connection, &empty, "#pick");
    let error = upload_files(
        &mut connection,
        &empty,
        &link,
        std::slice::from_ref(&file),
        deadline(),
    )
    .unwrap_err();
    assert_eq!(error.kind(), Some("no_file_chooser"), "{error:?}");
    assert!(
        error.message.contains("no <input type=file> on the page"),
        "{}",
        error.message
    );
    assert!(!format!("{error:?}").contains("snap -a"), "{error:?}");

    let attach = token_of(&mut connection, &empty, "#attach");
    let chosen = upload_files(
        &mut connection,
        &empty,
        &attach,
        std::slice::from_ref(&file),
        deadline(),
    )
    .unwrap();
    assert!(chosen.verified, "{chosen:?}");
    assert_eq!(chosen.via, tabref_page::FileInputVia::Chooser);
    assert_eq!(text_of(&mut connection, &empty, "#got"), name_of(&file));
    std::fs::remove_file(&file).unwrap();
}
