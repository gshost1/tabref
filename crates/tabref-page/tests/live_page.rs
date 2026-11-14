//! Live headless-Chrome checks of the snapshot reader and the actions on
//! local fixture pages. Opt-in: `TABREF_LIVE=1 cargo test -p tabref-page --test live_page`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use tabref_cdp::{CdpConnection, PageSession, wait_for_load};
use tabref_core::ErrorCode;
use tabref_page::{
    click, fill, read_element, select_option, set_checked, snapshot_page, upload_files,
};

use common::{connect_to, deadline, find, launch, live, open_tab, serve};

const FORM: &str = r#"<!doctype html><html><head><title>Form</title></head><body>
<label>Email <input id="email" type="email"></label>
<label>Due <input id="due" type="date"></label>
<label><input id="agree" type="checkbox"> I agree</label>
<label>Country <select id="country"><option>Norway</option><option value="se">Sweden</option></select></label>
<label>File <input id="file" type="file"></label>
<p>Read the <a href="/terms">terms</a> before you sign.</p>
<div onclick="this.textContent='Row opened'" style="cursor:pointer">Quarterly report</div>
</body></html>"#;

const COVERED: &str = r#"<!doctype html><html><head><title>Covered</title></head><body>
<a href="/landed">Hidden link</a>
<div style="position:fixed;inset:0;background:white">Overlay</div>
</body></html>"#;

const BELOW: &str = r#"<!doctype html><html><head><title>Below</title></head><body>
<div style="height:2400px">Spacer</div><a href="/landed">Learn more</a></body></html>"#;

const LANDED: &str =
    "<!doctype html><html><head><title>Landed</title></head><body><h1>Landed</h1></body></html>";

fn location(connection: &mut CdpConnection, page: &PageSession) -> String {
    connection
        .call(
            "Runtime.evaluate",
            json!({ "expression": "location.pathname", "returnByValue": true }),
            Some(&page.session_id),
            deadline(),
        )
        .ok()
        .and_then(|reply| {
            reply
                .pointer("/result/value")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default()
}

#[test]
fn form_controls_fill_select_check_upload_and_read_back() {
    if !live() {
        return;
    }
    let port = serve(&[("/", FORM)]);
    let chrome = launch();
    let mut connection = connect_to(&chrome);
    let page = open_tab(&mut connection, &format!("http://127.0.0.1:{port}/"));
    let snapshot = snapshot_page(&mut connection, &page, deadline()).unwrap();

    let (_, email) = find(&snapshot, "textfield", "Email");
    let filled = fill(
        &mut connection,
        &page,
        &email,
        "ada@example.test",
        deadline(),
    )
    .unwrap();
    assert!(filled.verified, "{filled:?}");

    let (_, due) = find(&snapshot, "datefield", "Due");
    let assigned = fill(&mut connection, &page, &due, "2026-11-08", deadline()).unwrap();
    assert_eq!(assigned.observed.as_deref(), Some("2026-11-08"));
    let malformed = fill(&mut connection, &page, &due, "08/11/2026", deadline()).unwrap_err();
    assert_eq!(malformed.code, ErrorCode::InvalidArgs);

    let (_, agree) = find(&snapshot, "checkbox", "I agree");
    assert!(set_checked(&mut connection, &page, &agree, true, deadline()).unwrap());
    assert_eq!(
        read_element(&mut connection, &page, &agree, deadline())
            .unwrap()
            .checked,
        Some(true)
    );

    let (_, country) = find(&snapshot, "combobox", "Country");
    assert_eq!(
        select_option(&mut connection, &page, &country, "sweden", deadline())
            .unwrap()
            .value,
        "se"
    );

    let (_, file_input) = find(&snapshot, "button", "File");
    let upload =
        std::env::temp_dir().join(format!("tabref-live-upload-{}.txt", std::process::id()));
    std::fs::write(&upload, b"hello").unwrap();
    let uploaded = upload_files(
        &mut connection,
        &page,
        &file_input,
        std::slice::from_ref(&upload),
        deadline(),
    )
    .unwrap();
    std::fs::remove_file(&upload).unwrap();
    assert!(uploaded.verified, "{uploaded:?}");
}

#[test]
fn text_reads_as_one_paragraph_and_a_click_listener_becomes_a_control() {
    if !live() {
        return;
    }
    let port = serve(&[("/", FORM)]);
    let chrome = launch();
    let mut connection = connect_to(&chrome);
    let page = open_tab(&mut connection, &format!("http://127.0.0.1:{port}/"));
    let snapshot = snapshot_page(&mut connection, &page, deadline()).unwrap();

    find(&snapshot, "paragraph", "Read the terms before you sign.");
    find(&snapshot, "link", "terms");
    let (row, token) = find(&snapshot, "group", "Quarterly report");
    assert!(
        row.presentation
            .available_actions
            .iter()
            .any(|action| action == "Click")
    );
    click(&mut connection, &page, &token, deadline()).unwrap();
    let after = snapshot_page(&mut connection, &page, deadline()).unwrap();
    find(&after, "group", "Row opened");
}

#[test]
fn a_covered_link_is_refused_and_a_link_below_the_fold_is_scrolled_to_and_followed() {
    if !live() {
        return;
    }
    let port = serve(&[
        ("/covered", COVERED),
        ("/below", BELOW),
        ("/landed", LANDED),
    ]);
    let chrome = launch();
    let mut connection = connect_to(&chrome);

    let covered = open_tab(&mut connection, &format!("http://127.0.0.1:{port}/covered"));
    let snapshot = snapshot_page(&mut connection, &covered, deadline()).unwrap();
    let (_, hidden) = find(&snapshot, "link", "Hidden link");
    let error = click(&mut connection, &covered, &hidden, deadline()).unwrap_err();
    assert_eq!(error.kind(), Some("cdp_click_intercepted"));
    assert_eq!(location(&mut connection, &covered), "/covered");

    let below = open_tab(&mut connection, &format!("http://127.0.0.1:{port}/below"));
    let snapshot = snapshot_page(&mut connection, &below, deadline()).unwrap();
    let (_, link) = find(&snapshot, "link", "Learn more");
    click(&mut connection, &below, &link, deadline()).unwrap();
    let started = std::time::Instant::now();
    while location(&mut connection, &below) != "/landed" {
        assert!(
            started.elapsed().as_secs() < 10,
            "the click never navigated"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    wait_for_load(&mut connection, &below, deadline()).unwrap();

    let stale = click(&mut connection, &below, &link, deadline()).unwrap_err();
    assert_eq!(stale.code, ErrorCode::StaleRef);
}

#[test]
fn a_cross_site_iframe_is_grafted_under_its_owner_and_clickable() {
    if !live() {
        return;
    }
    let inner = serve(&[(
        "/inner",
        r#"<!doctype html><title>Inner</title><button onclick="this.textContent='Inner done'">Inner go</button>"#,
    )]);
    let outer_html = format!(
        r#"<!doctype html><title>Outer</title><button>Outer go</button><iframe title="Embed" src="http://localhost:{inner}/inner"></iframe>"#
    );
    let outer = serve(&[("/", outer_html.as_str())]);
    let chrome = launch();
    let mut connection = connect_to(&chrome);
    let page = open_tab(&mut connection, &format!("http://127.0.0.1:{outer}/"));
    let snapshot = snapshot_page(&mut connection, &page, deadline()).unwrap();

    find(&snapshot, "button", "Outer go");
    let (_, inner_button) = find(&snapshot, "button", "Inner go");
    assert_ne!(
        inner_button.target_id, page.target_id,
        "the iframe is its own target"
    );
    let frame_session =
        tabref_cdp::attach_to_target(&mut connection, &inner_button.target_id, deadline()).unwrap();
    click(&mut connection, &frame_session, &inner_button, deadline()).unwrap();
    let after = snapshot_page(&mut connection, &page, deadline()).unwrap();
    find(&after, "button", "Inner done");
}
