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
    check_changes(
        imported,
        body,
        blind,
        &["sheet.disabled=true", "sheet.media.mediaText='not all'"],
    );
}

fn check_changes(imported: bool, body: &str, blind: bool, changes: &[&str]) {
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
        "<!doctype html><head>{style}</head><body><button id=go style='position:absolute;left:20px;top:20px'>Go</button><input id=toggle type=checkbox><span id=result>Result</span>{}</body>",
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
    evaluate(
        &mut connection,
        &page,
        "window.visibilityChecks=0; const original=Element.prototype.checkVisibility; Element.prototype.checkVisibility=function(...args){window.visibilityChecks++;return Reflect.apply(original,this,args)}",
    );
    for change in changes {
        evaluate(
            &mut connection,
            &page,
            "sheet.disabled=false; sheet.media.mediaText='all'; window.visibilityChecks=0",
        );
        let deadline = Deadline::after(30_000).unwrap();
        let watch = arm_capture_watch(&mut connection, &page, "opaque", deadline).unwrap();
        assert_eq!(watch.blind, blind, "{body}");
        if !blind {
            evaluate(
                &mut connection,
                &page,
                "if (visibilityChecks !== document.querySelectorAll('*').length) throw Error('visibility scanned more than once: '+visibilityChecks)",
            );
        }
        assert_eq!(
            check_capture_watch(&mut connection, &page, &watch, deadline) == "same",
            !blind
        );
        if *change == "hover" {
            connection
                .call(
                    "Input.dispatchMouseEvent",
                    json!({"type":"mouseMoved","x":25,"y":25}),
                    Some(&page.session_id),
                    deadline,
                )
                .unwrap();
        } else {
            evaluate(&mut connection, &page, change);
        }
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
        ".a{b:url(x\"y)}@container (min-width:1px){.x{color:red}}.c{d:\")}",
        ".a{b:url(x/*)}@container (min-width:1px){.x{color:red}}.c{d:e*/)}",
        ".a{--v:\"\u{000C}}@container (min-width:1px){.x{color:red}}.c{--w:\"}",
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
fn opaque_container_queries_are_blind() {
    check_opaque_sheet(false, "@container(width>1px){#go{visibility:hidden}}", true);
}

#[test]
fn opaque_untracked_media_are_blind() {
    check_opaque_sheet(false, "@media(hover:hover){#go{visibility:hidden}}", true);
}

#[test]
fn opaque_relational_pseudo_classes_are_blind() {
    check_opaque_sheet(
        false,
        "body:has(#toggle:checked) #go{visibility:hidden}",
        true,
    );
}

#[test]
fn malformed_and_huge_opaque_sheets_are_blind() {
    for body in ["#go{visibility hidden}", "#go{visibility:}"] {
        check_opaque_sheet(false, body, true);
    }
    check_opaque_sheet(false, &" ".repeat(1_000_001), true);
    check_opaque_sheet(false, &"a{}".repeat(10_001), true);
}

#[test]
fn opaque_state_rules_invalidate_the_watch() {
    for (body, change) in [
        ("#go:hover{visibility:hidden}", "hover"),
        (
            "#toggle:checked+#result{visibility:hidden}",
            "document.querySelector('#toggle').checked=true",
        ),
        (
            "#go:focus{visibility:hidden}",
            "document.querySelector('#go').focus()",
        ),
    ] {
        check_changes(false, body, false, &[change]);
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
    daemon_reuse("button{color:red}", None, true);
}

#[test]
fn opaque_state_visibility_changes_do_not_reuse_daemon_captures() {
    for (css, change) in [
        ("button:hover{visibility:hidden}", "hover"),
        (
            "input:checked+button{visibility:hidden}",
            "document.querySelector('input').checked=true",
        ),
        (
            "button:focus{visibility:hidden}",
            "document.querySelector('button').focus()",
        ),
    ] {
        daemon_reuse(css, Some(change), false);
    }
}

#[test]
fn unclassifiable_opaque_sheets_do_not_reuse_daemon_captures() {
    for css in [
        "button{visibility hidden}",
        "@container(width>1px){button{visibility:hidden}}",
        "@media(hover:hover){button{visibility:hidden}}",
        "body:has(input:checked) button{visibility:hidden}",
    ] {
        daemon_reuse(css, None, false);
    }
    daemon_reuse(&" ".repeat(1_000_001), None, false);
}

fn daemon_reuse(css: &str, change: Option<&str>, reuse: bool) {
    if !common::live() {
        return;
    }
    let css = stylesheet(css);
    let html = format!(
        "<!doctype html><link rel=stylesheet href='http://127.0.0.1:{css}/style.css'><input type=checkbox><button style='position:absolute;left:20px;top:20px'>Go</button>"
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
    if let Some(change) = change {
        let deadline = Deadline::after(30_000).unwrap();
        let mut connection = connect(&harness.browser.endpoint().unwrap(), deadline).unwrap();
        let target = list_page_targets(&mut connection, deadline)
            .unwrap()
            .remove(0);
        let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
        if change == "hover" {
            connection
                .call(
                    "Input.dispatchMouseEvent",
                    json!({"type":"mouseMoved","x":25,"y":25}),
                    Some(&page.session_id),
                    deadline,
                )
                .unwrap();
        } else {
            evaluate(&mut connection, &page, change);
        }
    }
    let find = run(&["find", "Go"]);
    assert_eq!(
        snap["data"]["snapshot_id"] == find["data"]["snapshot_id"],
        reuse,
        "{css}: {change:?}"
    );
    run(&["daemon", "stop"]);
}
