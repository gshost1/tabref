//! Opt-in: TABREF_LIVE=1 cargo test -p tabref --test live_capture_stylesheets.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;

use serde_json::json;
use tabref_cdp::{CdpConnection, PageSession, attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;
use tabref_page::{arm_capture_watch, check_capture_watch};

fn stylesheet() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 8192];
            let _ = stream.read(&mut request);
            let body = "#go { visibility: visible }";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    port
}

fn evaluate(connection: &mut CdpConnection, page: &PageSession, expression: &str) {
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({ "expression": expression, "awaitPromise": true, "returnByValue": true }),
            Some(&page.session_id),
            Deadline::after(30_000).unwrap(),
        )
        .unwrap();
    assert!(reply.get("exceptionDetails").is_none(), "{reply}");
}

fn check_opaque_sheet(imported: bool) {
    if !common::live() {
        return;
    }
    let css = stylesheet();
    let href = format!("http://127.0.0.1:{css}/style.css");
    let style = if imported {
        format!("<style>@import url('{href}');</style>")
    } else {
        format!("<link rel=stylesheet href='{href}'>")
    };
    let html = format!(
        "<!doctype html><head>{style}</head><body><button id=go>Go</button>{}</body>",
        "<span>Row</span>".repeat(1_200)
    );
    let port = common::serve(&[("/", &html)]);
    let harness = common::harness("opaque-stylesheet");
    let deadline = Deadline::after(30_000).unwrap();
    let mut connection = connect(&harness.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .remove(0);
    let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
    connection
        .call("Page.enable", json!({}), Some(&page.session_id), deadline)
        .unwrap();
    connection
        .call(
            "Page.navigate",
            json!({ "url": format!("http://127.0.0.1:{port}/") }),
            Some(&page.session_id),
            deadline,
        )
        .unwrap();
    evaluate(
        &mut connection,
        &page,
        "document.readyState === 'complete' ? true : new Promise(resolve => addEventListener('load', resolve, {once:true}))",
    );
    let sheet = if imported {
        "document.styleSheets[0].cssRules[0].styleSheet"
    } else {
        "document.styleSheets[0]"
    };
    evaluate(
        &mut connection,
        &page,
        &format!(
            "window.sheet={sheet}; let denied=false; try {{ sheet.cssRules; }} catch(e) {{ denied=e.name==='SecurityError'; }} if (!denied) throw Error('sheet rules were readable')"
        ),
    );
    for change in ["sheet.disabled=true", "sheet.media.mediaText='not all'"] {
        let deadline = Deadline::after(30_000).unwrap();
        let watch = arm_capture_watch(&mut connection, &page, "opaque", deadline).unwrap();
        assert!(!watch.blind);
        assert_eq!(
            check_capture_watch(&mut connection, &page, &watch, deadline),
            "same"
        );
        evaluate(&mut connection, &page, change);
        assert_eq!(
            check_capture_watch(&mut connection, &page, &watch, deadline),
            "state"
        );
    }
}

#[test]
fn an_opaque_link_sheet_is_watchable_but_disabled_and_media_changes_are_not_replayed() {
    check_opaque_sheet(false);
}

#[test]
fn an_opaque_import_in_a_readable_sheet_is_watched_too() {
    check_opaque_sheet(true);
}
