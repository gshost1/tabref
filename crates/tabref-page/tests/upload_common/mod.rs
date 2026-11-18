//! Helpers the live upload suites share: a token for a CSS-picked node,
//! the file names an input holds, and fixture files to upload.

#![allow(clippy::unwrap_used, clippy::expect_used, dead_code)]

use std::path::PathBuf;

use serde_json::{Value, json};
use tabref_cdp::{CdpConnection, PageSession};
use tabref_page::ElementToken;

use super::common::deadline;

pub fn token_of(connection: &mut CdpConnection, page: &PageSession, css: &str) -> ElementToken {
    let session = Some(page.session_id.as_str());
    let document = connection
        .call(
            "DOM.getDocument",
            json!({ "depth": 0 }),
            session,
            deadline(),
        )
        .unwrap();
    let root = document.pointer("/root/nodeId").unwrap().clone();
    let found = connection
        .call(
            "DOM.querySelector",
            json!({ "nodeId": root, "selector": css }),
            session,
            deadline(),
        )
        .unwrap();
    let described = connection
        .call(
            "DOM.describeNode",
            json!({ "nodeId": found["nodeId"] }),
            session,
            deadline(),
        )
        .unwrap();
    let tree = connection
        .call("Page.getFrameTree", json!({}), session, deadline())
        .unwrap();
    let frame = tree.pointer("/frameTree/frame").unwrap();
    ElementToken {
        target_id: page.target_id.clone(),
        frame_id: frame["id"].as_str().unwrap().to_owned(),
        loader_id: frame["loaderId"].as_str().unwrap().to_owned(),
        backend_node_id: described
            .pointer("/node/backendNodeId")
            .unwrap()
            .as_i64()
            .unwrap(),
    }
}

pub fn files_of(connection: &mut CdpConnection, page: &PageSession, css: &str) -> Vec<String> {
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": format!(
                    "Array.from(document.querySelector({css:?}).files).map((f) => f.name)"
                ),
                "returnByValue": true,
            }),
            Some(&page.session_id),
            deadline(),
        )
        .unwrap();
    reply
        .pointer("/result/value")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect()
}

/// The text of the element `css` picks.
pub fn text_of(connection: &mut CdpConnection, page: &PageSession, css: &str) -> String {
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": format!("document.querySelector({css:?}).textContent"),
                "returnByValue": true,
            }),
            Some(&page.session_id),
            deadline(),
        )
        .unwrap();
    reply
        .pointer("/result/value")
        .and_then(Value::as_str)
        .unwrap()
        .to_owned()
}

pub fn fixture(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("tabref-live-up-{}-{name}", std::process::id()));
    std::fs::write(&path, b"hello").unwrap();
    path
}

pub fn name_of(path: &std::path::Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

/// The names of the files every file input holds, in the page and in the
/// open shadow root of `#chat` when there is one.
pub fn held_files(connection: &mut CdpConnection, page: &PageSession) -> Vec<String> {
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({
                "expression": "(() => { const all = [...document.querySelectorAll('input[type=file]')]; const host = document.getElementById('chat'); if (host) all.push(...host.shadowRoot.querySelectorAll('input[type=file]')); return all.flatMap((input) => [...input.files].map((file) => file.name)); })()",
                "returnByValue": true,
            }),
            Some(&page.session_id),
            deadline(),
        )
        .unwrap();
    reply
        .pointer("/result/value")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_owned())
        .collect()
}
