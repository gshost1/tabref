use serde_json::json;
use tabref_core::ErrorCode;

use super::upload_files;
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, resolved, session, token};
use crate::act::file_input_choice::FileInputVia;
use tabref_cdp::ScriptedTransport;

fn fixture_file(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("tabref-upload-{}-{name}", std::process::id()));
    std::fs::write(&path, b"hello").unwrap();
    path
}

fn listed(candidates: serde_json::Value) -> serde_json::Value {
    json!({ "result": { "type": "object", "value": { "top": 3, "candidates": candidates } } })
}

#[test]
fn files_are_set_on_the_input_and_read_back() {
    let file = fixture_file("report.txt");
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    let transport = resolved(ScriptedTransport::new())
        .expect(
            "Runtime.callFunctionOn",
            listed(json!([{ "index": 0, "name": "input#file", "self": true, "depth": 0 }])),
        )
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "objectId": "I" } }),
        )
        .expect("DOM.setFileInputFiles", json!({}))
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": [name] } }),
        )
        .expect("Runtime.releaseObject", json!({}))
        .expect("Runtime.releaseObject", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let outcome = upload_files(
        &mut connection,
        &session(),
        &token(5),
        std::slice::from_ref(&file),
        deadline(),
    )
    .unwrap();
    std::fs::remove_file(&file).unwrap();

    assert!(outcome.verified);
    assert_eq!(outcome.input, "input#file");
    assert_eq!(outcome.via, FileInputVia::Itself);
    let sent = sent.lock().unwrap();
    let set = sent
        .iter()
        .find(|request| request["method"] == "DOM.setFileInputFiles")
        .unwrap();
    assert_eq!(set["params"]["objectId"], "I");
    assert!(
        set["params"]["files"][0]
            .as_str()
            .unwrap()
            .ends_with("report.txt")
    );
}

fn refused(case: &str, candidates: serde_json::Value, count: usize) -> tabref_core::AppError {
    let files: Vec<_> = (0..count)
        .map(|at| fixture_file(&format!("{case}-{at}.txt")))
        .collect();
    let transport = resolved(ScriptedTransport::new())
        .expect("Runtime.callFunctionOn", listed(candidates))
        .expect("Runtime.releaseObject", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    let error =
        upload_files(&mut connection, &session(), &token(5), &files, deadline()).unwrap_err();
    for file in &files {
        std::fs::remove_file(file).unwrap();
    }
    assert!(
        !sent
            .lock()
            .unwrap()
            .iter()
            .any(|request| request["method"] == "DOM.setFileInputFiles")
    );
    error
}

#[test]
fn two_equally_near_inputs_are_refused_and_named() {
    let error = refused(
        "ambiguous",
        json!([
            { "index": 0, "name": "input.dz-hidden-input", "depth": 3 },
            { "index": 1, "name": "input#avatar", "depth": 3 },
        ]),
        1,
    );
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(error.kind(), Some("ambiguous_file_input"));
    assert!(
        error
            .message
            .contains("input.dz-hidden-input, input#avatar")
    );
}

#[test]
fn two_files_for_a_single_file_input_are_refused() {
    let error = refused(
        "single",
        json!([{ "index": 0, "name": "input#avatar", "depth": 1, "multiple": false }]),
        2,
    );
    assert_eq!(error.code, ErrorCode::InvalidArgs);
    assert_eq!(error.message, "input#avatar takes one file, not 2");
}

#[test]
fn a_disabled_input_is_refused() {
    let error = refused(
        "disabled",
        json!([{ "index": 0, "name": "input#off", "self": true, "depth": 0, "disabled": true }]),
        1,
    );
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.kind(), Some("cdp_not_file_input"));
}

#[test]
fn a_missing_file_is_invalid_args_before_the_page_is_touched() {
    let mut connection = CdpConnection::new(ScriptedTransport::new().boxed());
    let missing = std::env::temp_dir().join("tabref-upload-definitely-missing.bin");

    let error = upload_files(
        &mut connection,
        &session(),
        &token(5),
        &[missing],
        deadline(),
    )
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::InvalidArgs);
}

fn failed_set(message: &str) -> tabref_core::AppError {
    let file = fixture_file(&format!("setfail-{}.txt", message.len()));
    let transport = resolved(ScriptedTransport::new())
        .expect(
            "Runtime.callFunctionOn",
            listed(json!([{ "index": 0, "name": "input.dz-hidden-input", "depth": 2 }])),
        )
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "objectId": "I" } }),
        )
        .expect_error("DOM.setFileInputFiles", -32000, message)
        .expect("Runtime.releaseObject", json!({}))
        .expect("Runtime.releaseObject", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());
    let error = upload_files(
        &mut connection,
        &session(),
        &token(5),
        std::slice::from_ref(&file),
        deadline(),
    )
    .unwrap_err();
    std::fs::remove_file(&file).unwrap();
    error
}

#[test]
fn a_failed_set_names_the_input_not_the_target() {
    let gone = failed_set("Node is detached from document");
    assert_eq!(gone.code, ErrorCode::StaleRef, "{gone:?}");
    assert!(
        gone.message.contains("input.dz-hidden-input"),
        "{}",
        gone.message
    );
    assert!(!gone.message.contains("DOM node 5"), "{}", gone.message);
    let other = failed_set("Not allowed");
    assert!(
        other.message.contains("input.dz-hidden-input"),
        "{}",
        other.message
    );
}

#[test]
fn an_input_across_a_form_boundary_is_refused_and_named() {
    let error = refused(
        "fenced",
        json!([{ "index": 0, "name": "input#photo", "depth": 2, "crosses": true }]),
        1,
    );
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(error.kind(), Some("ambiguous_file_input"));
    assert!(error.message.contains("input#photo"), "{}", error.message);
}
