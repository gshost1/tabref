use serde_json::json;
use tabref_core::ErrorCode;

use super::upload_files;
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, resolved, session, token};
use tabref_cdp::ScriptedTransport;

fn fixture_file(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("tabref-upload-{}-{name}", std::process::id()));
    std::fs::write(&path, b"hello").unwrap();
    path
}

#[test]
fn files_are_set_on_the_input_and_read_back() {
    let file = fixture_file("report.txt");
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    let transport = resolved(ScriptedTransport::new())
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": { "multiple": false } } }),
        )
        .expect("DOM.setFileInputFiles", json!({}))
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": [name] } }),
        )
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
    let sent = sent.lock().unwrap();
    let set = sent
        .iter()
        .find(|request| request["method"] == "DOM.setFileInputFiles")
        .unwrap();
    assert_eq!(set["params"]["backendNodeId"], 5);
    assert!(
        set["params"]["files"][0]
            .as_str()
            .unwrap()
            .ends_with("report.txt")
    );
}

#[test]
fn a_node_that_is_not_a_file_input_is_refused_before_anything_is_set() {
    let file = fixture_file("x.txt");
    let transport = resolved(ScriptedTransport::new())
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": { "error": "not_file_input" } } }),
        )
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
