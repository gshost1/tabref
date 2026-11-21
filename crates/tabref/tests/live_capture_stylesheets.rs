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

fn stylesheet(body: &str) -> u16 {
    let body = body.to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 8192];
            let _ = stream.read(&mut request);
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

fn check_opaque_sheet(imported: bool, body: &str, blind: bool) {
    if !common::live() {
        return;
    }
    let css = stylesheet(body);
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
        evaluate(
            &mut connection,
            &page,
            "sheet.disabled=false; sheet.media.mediaText='all'",
        );
        let deadline = Deadline::after(30_000).unwrap();
        let watch = arm_capture_watch(&mut connection, &page, "opaque", deadline).unwrap();
        assert_eq!(watch.blind, blind, "{body}");
        assert_eq!(
            check_capture_watch(&mut connection, &page, &watch, deadline) == "same",
            !blind
        );
        evaluate(&mut connection, &page, change);
        assert_ne!(
            check_capture_watch(&mut connection, &page, &watch, deadline),
            "same"
        );
    }
}

#[test]
fn a_safe_opaque_link_sheet_is_watched() {
    check_opaque_sheet(false, "#go { visibility: visible }", false);
}

#[test]
fn a_safe_opaque_import_in_a_readable_sheet_is_watched() {
    check_opaque_sheet(true, "#go { visibility: visible }", false);
}

#[test]
fn unsafe_opaque_rules_are_blind() {
    for body in [
        "@container(width>1px){#go{color:red}}",
        "body:has(button){color:red}",
        "@unknown something {#go{color:red}}",
        "@media(hover:hover){#go{color:red}}",
        "@media(min-resolution:2dppx){#go{color:red}}",
        "@property --x{syntax:'*';inherits:true}",
    ] {
        check_opaque_sheet(false, body, true);
    }
}

#[test]
fn opaque_imports_are_classified_recursively() {
    if !common::live() {
        return;
    }
    for (body, blind) in [
        ("button{color:red}", false),
        ("body:has(button){color:red}", true),
    ] {
        let child = stylesheet(body);
        check_opaque_sheet(
            false,
            &format!("@import url('http://127.0.0.1:{child}/child.css');"),
            blind,
        );
    }
    check_opaque_sheet(
        false,
        "@import url('http://127.0.0.1:1/missing.css');",
        true,
    );
}

#[test]
fn a_safe_opaque_sheet_allows_daemon_read_reuse() {
    if !common::live() {
        return;
    }
    let css = stylesheet("@media(prefers-color-scheme:dark){button{color:white}}button{color:red}");
    let html = format!(
        "<!doctype html><link rel=stylesheet href='http://127.0.0.1:{css}/style.css'><button>Go</button>"
    );
    let port = common::serve(&[("/", &html)]);
    let harness = common::harness("opaque-reuse");
    let run = |args: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_tabref"))
            .args([
                "--cdp",
                &harness.browser.port.to_string(),
                "--json",
                "--timeout",
                "30000",
            ])
            .args(args)
            .env("TABREF_HOME", &harness.home)
            .env_remove("TABREF_NO_DAEMON")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    run(&["open", &format!("http://127.0.0.1:{port}/")]);
    let snap = run(&["snap"]);
    let find = run(&["find", "Go"]);
    assert_eq!(snap["data"]["snapshot_id"], find["data"]["snapshot_id"]);
    run(&["daemon", "stop"]);
}
