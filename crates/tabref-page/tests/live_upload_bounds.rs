//! Live checks that `upload_files` never sends files across a form or a
//! widget's shadow root to an input the target does not belong to, still
//! reaches the hidden inputs real drop zones add, and refuses when the
//! inputs change between the search and the upload. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref-page --test live_upload_bounds`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod upload_common;

use tabref_core::{AppError, ErrorCode};
use tabref_page::upload_files;

use common::{connect_to, deadline, launch, live, open_tab, serve};
use upload_common::{fixture, held_files, name_of, token_of};

const SAME_FORM: &str = r##"<!doctype html><html><head><title>Same form</title></head><body>
<form id="issue"><textarea name="body"></textarea>
<div class="drop"><a href="#" id="pick">Attach files</a></div><button>Comment</button></form>
<script>
const input = document.createElement('input');
input.type = 'file';
input.multiple = true;
input.hidden = true;
input.id = 'attach';
document.getElementById('issue').appendChild(input);
</script></body></html>"##;

const BODY_DROPZONE: &str = r##"<!doctype html><html><head><title>Body dropzone</title></head><body>
<form id="report"><textarea name="body"></textarea>
<div class="dropzone"><a href="#" id="pick">select more files…</a></div><button>Submit report</button></form>
<script>
const input = document.createElement('input');
input.type = 'file';
input.multiple = true;
input.className = 'dz-hidden-input';
input.style.cssText = 'visibility:hidden;position:absolute;top:0;left:0;height:0;width:0';
input.setAttribute('form', 'report');
document.body.appendChild(input);
</script></body></html>"##;

const OUTSIDE_FORM: &str = r#"<!doctype html><title>Comment and avatar</title>
<header><input type="file" id="avatar" hidden></header>
<form id="comment"><button type="button" id="attach">Attach</button></form>"#;

const SIBLING_FORMS: &str = r##"<!doctype html><html><head><title>Card</title></head><body>
<div class="card">
<form id="message"><textarea name="body"></textarea><button id="send">Attach and send</button></form>
<form id="avatar"><input type="file" id="photo" name="photo"></form>
</div></body></html>"##;

const CHAT_WIDGET: &str = r##"<!doctype html><html><head><title>Widget</title></head><body>
<main><button id="plain">Upload</button></main>
<div id="chat"></div>
<script>
const root = document.getElementById('chat').attachShadow({ mode: 'open' });
root.innerHTML = '<input type="file" id="chat-file" hidden><button>Chat</button>';
</script></body></html>"##;

const RERENDER: &str = r##"<!doctype html><html><head><title>Rerender</title></head><body>
<form id="form"><a href="#" id="pick">Attach</a><input type="file" id="real" hidden></form>
<form id="other"></form>
<script>
const real = document.getElementById('real');
let once = false;
Object.defineProperty(real, 'multiple', {
  get() {
    if (!once) {
      once = true;
      queueMicrotask(() => {
        const decoy = document.createElement('input');
        decoy.type = 'file';
        decoy.id = 'decoy';
        document.getElementById('form').prepend(decoy);
      });
    }
    return false;
  },
});
</script></body></html>"##;

fn upload_to(page_path: &str, html: &'static str, css: &str) -> Result<String, AppError> {
    let port = serve(&[(page_path, html)]);
    let chrome = launch();
    let mut connection = connect_to(&chrome);
    let page = open_tab(
        &mut connection,
        &format!("http://127.0.0.1:{port}{page_path}"),
    );
    let file = fixture(&format!("bounds{}.txt", page_path.replace('/', "-")));
    let target = token_of(&mut connection, &page, css);
    let result = upload_files(
        &mut connection,
        &page,
        &target,
        std::slice::from_ref(&file),
        deadline(),
    );
    let name = name_of(&file);
    let held = held_files(&mut connection, &page);
    std::fs::remove_file(&file).unwrap();
    let outcome = result.map(|uploaded| {
        assert!(uploaded.page_reports_requested_files, "{uploaded:?}");
        uploaded.input
    });
    if outcome.is_err() {
        assert!(
            held.is_empty(),
            "files were set although the upload was refused: {held:?}"
        );
    } else {
        assert_eq!(held, vec![name]);
    }
    outcome
}

#[test]
fn a_hidden_input_appended_to_the_same_form_is_used() {
    if !live() {
        return;
    }
    let outcome = upload_to("/same", SAME_FORM, "#pick");
    assert_eq!(outcome.unwrap(), "input#attach");
}

#[test]
fn a_body_appended_input_owned_by_the_form_is_used() {
    if !live() {
        return;
    }
    let outcome = upload_to("/body", BODY_DROPZONE, "#pick");
    assert_eq!(outcome.unwrap(), "input.dz-hidden-input");
}

#[test]
fn a_target_inside_a_form_cannot_use_a_formless_input() {
    if !live() {
        return;
    }
    let error = upload_to("/unowned", OUTSIDE_FORM, "#attach").unwrap_err();
    assert_eq!(error.kind(), Some("ambiguous_file_input"));
    assert!(error.message.contains("input#avatar"), "{error:?}");
}

#[test]
fn a_sibling_forms_input_in_a_shared_card_is_refused() {
    if !live() {
        return;
    }
    let outcome = upload_to("/card", SIBLING_FORMS, "#send");
    let error = outcome.unwrap_err();
    assert_eq!(error.code, ErrorCode::AmbiguousTarget, "{error:?}");
    assert_eq!(error.kind(), Some("ambiguous_file_input"));
    assert!(error.message.contains("input#photo"), "{}", error.message);
}

#[test]
fn a_chat_widgets_input_inside_its_own_shadow_root_is_refused() {
    if !live() {
        return;
    }
    let outcome = upload_to("/widget", CHAT_WIDGET, "#plain");
    let error = outcome.unwrap_err();
    assert_eq!(error.code, ErrorCode::AmbiguousTarget, "{error:?}");
    assert!(
        error.message.contains("input#chat-file"),
        "{}",
        error.message
    );
}

#[test]
fn inputs_that_change_between_the_search_and_the_upload_are_refused() {
    if !live() {
        return;
    }
    let outcome = upload_to("/rerender", RERENDER, "#pick");
    let error = outcome.unwrap_err();
    assert_eq!(error.code, ErrorCode::StaleRef, "{error:?}");
}
