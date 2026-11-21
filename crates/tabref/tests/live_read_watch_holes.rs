//! Opt-in: TABREF_LIVE=1 cargo test -p tabref --test live_read_watch_holes.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

use serde_json::{Value, json};
use tabref_cdp::{CdpConnection, PageSession, attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;

fn run(harness: &common::Harness, args: &[&str], fresh: bool) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args([
            "--cdp",
            &harness.browser.port.to_string(),
            "--json",
            "--timeout",
            "30000",
        ])
        .args(args)
        .env("TABREF_HOME", &harness.home)
        .env_remove("TABREF_NO_DAEMON");
    if fresh {
        command.env("TABREF_NO_DAEMON", "1");
    }
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

fn evaluate(connection: &mut CdpConnection, page: &PageSession, expression: &str) {
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({"expression":expression,"awaitPromise":true,"returnByValue":true}),
            Some(&page.session_id),
            Deadline::after(30_000).unwrap(),
        )
        .unwrap();
    assert!(reply.get("exceptionDetails").is_none(), "{reply}");
}

fn setup(html: &str) -> (common::Harness, CdpConnection, PageSession) {
    let harness = common::harness("read-watch-holes");
    let port = common::serve(&[("/", html)]);
    run(
        &harness,
        &["open", &format!("http://127.0.0.1:{port}/")],
        false,
    );
    let deadline = Deadline::after(30_000).unwrap();
    let mut connection = connect(&harness.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .into_iter()
        .find(|target| target.url.starts_with("http://127.0.0.1:"))
        .unwrap();
    let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
    (harness, connection, page)
}

fn lines(value: &Value) -> String {
    value["data"]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| {
            line.as_str()
                .unwrap()
                .split_whitespace()
                .map(|word| {
                    if word.strip_prefix("[e").is_some_and(|tail| {
                        !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit())
                    }) {
                        "["
                    } else {
                        word
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn compare(harness: &common::Harness, command: &str) {
    let cached = run(harness, &[command], false);
    let fresh = run(harness, &[command], true);
    assert_eq!(
        lines(&cached),
        lines(&fresh),
        "daemon {command} replayed a stale capture"
    );
}

fn css_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = [0; 8192];
            let _ = stream.read(&mut request);
            let body = "button::after{content:'Light'}@media(prefers-color-scheme:dark){button::after{content:'Dark'}}@media(prefers-reduced-motion:reduce){button::after{content:'Reduced'}}";
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/css\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

#[test]
fn opaque_media_changes_do_not_replay_labels() {
    if !common::live() {
        return;
    }
    let css = css_server();
    for (name, value) in [
        ("prefers-color-scheme", "dark"),
        ("prefers-reduced-motion", "reduce"),
    ] {
        for command in ["snap", "read"] {
            let html = format!(
                "<!doctype html><link rel=stylesheet href='http://127.0.0.1:{css}/style.css'><button>Mode </button>"
            );
            let (harness, mut connection, page) = setup(&html);
            connection.call("Emulation.setEmulatedMedia", json!({"features":[{"name":name,"value":if value == "dark" { "light" } else { "no-preference" }}]}), Some(&page.session_id), Deadline::after(30_000).unwrap()).unwrap();
            run(&harness, &["snap"], false);
            connection
                .call(
                    "Emulation.setEmulatedMedia",
                    json!({"features":[{"name":name,"value":value}]}),
                    Some(&page.session_id),
                    Deadline::after(30_000).unwrap(),
                )
                .unwrap();
            compare(&harness, command);
            run(&harness, &["daemon", "stop"], false);
        }
    }
}

fn edit_case(html: &str, edit: &str) {
    for command in ["snap", "read"] {
        let (harness, mut connection, page) = setup(html);
        run(&harness, &["snap"], false);
        evaluate(&mut connection, &page, edit);
        compare(&harness, command);
        run(&harness, &["daemon", "stop"], false);
    }
}

#[test]
fn registering_a_property_does_not_replay_generated_content() {
    if !common::live() {
        return;
    }
    edit_case(
        "<!doctype html><style>button::after{content:var(--read-hole,'Before')}</style><button>Label </button>",
        "CSS.registerProperty({name:'--read-hole',syntax:'<string>',inherits:false,initialValue:'\"After\"'})",
    );
}

#[test]
fn scoped_registry_internals_do_not_replay_labels() {
    if !common::live() {
        return;
    }
    edit_case(
        "<!doctype html><div id=host></div><script>const registry=new CustomElementRegistry();registry.define('x-scoped',class extends HTMLElement{constructor(){super();this.ins=this.attachInternals();this.ins.role='button';this.ins.ariaLabel='Before';this.tabIndex=0}});const root=host.attachShadow({mode:'open',customElementRegistry:registry});root.innerHTML='<x-scoped></x-scoped>';window.control=root.firstChild;</script>",
        "control.ins.ariaLabel='After'",
    );
}

#[test]
fn manual_slot_reordering_does_not_replay_reading_order() {
    if !common::live() {
        return;
    }
    edit_case(
        "<!doctype html><div id=host><button id=x>First</button><button id=y>Second</button></div><script>const root=host.attachShadow({mode:'open',slotAssignment:'manual'});root.innerHTML='<slot></slot>';window.slot=root.firstChild;slot.assign(x,y);</script>",
        "slot.assign(y,x)",
    );
}

#[test]
fn adding_a_font_does_not_replay_container_query_content() {
    if !common::live() {
        return;
    }
    edit_case(
        "<!doctype html><style>#box{container-type:inline-size;width:10ch;font:20px ReadHole,monospace}button::after{content:'Narrow'}@container(min-width:116px){button::after{content:'Wide'}}</style><div id=box><button>Size </button></div><script>window.face=new FontFace('ReadHole','local(Arial),local(\"Liberation Sans\"),local(\"DejaVu Sans\")');</script>",
        "face.load().then(()=>{document.fonts.add(face);return document.fonts.ready}).then(()=>new Promise(requestAnimationFrame))",
    );
}

#[test]
fn until_found_content_is_never_retained() {
    if !common::live() {
        return;
    }
    for command in ["snap", "read"] {
        let (harness, mut connection, page) = setup(
            "<!doctype html><p>Visible</p><section hidden=until-found>Secret phrase</section>",
        );
        let before = run(&harness, &["snap"], false);
        evaluate(&mut connection, &page, "window.find('Secret phrase')");
        let cached = run(&harness, &[command], false);
        let fresh = run(&harness, &[command], true);
        run(&harness, &["daemon", "stop"], false);
        assert_eq!(lines(&cached), lines(&fresh));
        if command == "snap" {
            assert_ne!(
                before["data"]["snapshot_id"], cached["data"]["snapshot_id"],
                "retained hidden=until-found content"
            );
        }
    }
}
