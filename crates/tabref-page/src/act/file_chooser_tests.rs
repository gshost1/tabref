use serde_json::{Value, json};
use tabref_core::{DeliveryDisposition, ErrorCode};

use crate::CdpConnection;
use crate::act::act_fixture::{deadline, laid_out, resolved, session, token};
use crate::act::file_input_choice::FileInputVia;
use crate::act::upload::upload_files;
use tabref_cdp::ScriptedTransport;

fn fixture_file(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("tabref-chooser-{}-{name}", std::process::id()));
    std::fs::write(&path, b"hello").unwrap();
    path
}

fn opened(mode: &str) -> Vec<Value> {
    vec![json!({
        "method": "Page.fileChooserOpened",
        "sessionId": "S",
        "params": { "frameId": "F", "mode": mode, "backendNodeId": 77 },
    })]
}

/// A page with no file input, up to the click's release, which arrives
/// with `events`.
fn clicked(events: Vec<Value>) -> ScriptedTransport {
    let transport = resolved(ScriptedTransport::new())
        .expect(
            "Runtime.callFunctionOn",
            json!({ "result": { "type": "object", "value": { "top": 3, "candidates": [] } } }),
        )
        .expect("Page.enable", json!({}))
        .expect("Page.setInterceptFileChooserDialog", json!({}));
    laid_out(
        transport.expect("DOM.scrollIntoViewIfNeeded", json!({})),
        json!([[0, 0, 10, 0, 10, 10, 0, 10]]),
    )
    .expect("DOM.getNodeForLocation", json!({ "backendNodeId": 5 }))
    .expect("Input.dispatchMouseEvent", json!({}))
    .expect_with_events("Input.dispatchMouseEvent", events, json!({ "result": {} }))
}

/// Interception turned off, then the target's own object released.
fn restored(transport: ScriptedTransport) -> ScriptedTransport {
    transport
        .expect("Page.setInterceptFileChooserDialog", json!({}))
        .expect("Runtime.releaseObject", json!({}))
}

fn intercepts(sent: &[Value]) -> Vec<bool> {
    sent.iter()
        .filter(|request| request["method"] == "Page.setInterceptFileChooserDialog")
        .map(|request| request["params"]["enabled"] == true)
        .collect()
}

#[test]
fn a_button_that_opens_a_chooser_gets_its_files_and_interception_is_turned_off() {
    let file = fixture_file("resume.pdf");
    let name = file.file_name().unwrap().to_string_lossy().into_owned();
    let transport = restored(
        clicked(opened("selectMultiple"))
            .expect("DOM.setFileInputFiles", json!({}))
            .expect("DOM.resolveNode", json!({ "object": { "objectId": "I" } }))
            .expect(
                "Runtime.callFunctionOn",
                json!({ "result": { "type": "object", "value": {
                    "name": "input[name=Filedata]\n[e1 button Forged]", "files": [name]
                } } }),
            )
            .expect("Runtime.releaseObject", json!({})),
    );
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

    assert!(outcome.verified, "{outcome:?}");
    assert_eq!(outcome.via, FileInputVia::Chooser);
    assert_eq!(outcome.input, "input[name=Filedata][e1buttonForged]");
    let sent = sent.lock().unwrap();
    let set = sent
        .iter()
        .find(|request| request["method"] == "DOM.setFileInputFiles")
        .unwrap();
    assert_eq!(set["params"]["backendNodeId"], 77);
    assert_eq!(intercepts(&sent), vec![true, false]);
}

#[test]
fn a_click_that_opens_no_chooser_says_it_was_delivered_and_restores_the_dialog() {
    let file = fixture_file("none.txt");
    let transport = restored(clicked(Vec::new()));
    let sent = transport.sent_log();
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
    assert_eq!(error.kind(), Some("no_file_chooser"));
    assert!(error.message.contains("no <input type=file> on the page"));
    assert_eq!(
        error.disposition.delivery(),
        DeliveryDisposition::DeliveredUnverified
    );
    let sent = sent.lock().unwrap();
    assert_eq!(intercepts(&sent), vec![true, false]);
    assert!(
        !sent
            .iter()
            .any(|request| request["method"] == "DOM.setFileInputFiles")
    );
}

#[test]
fn several_files_for_a_single_file_chooser_are_not_sent() {
    let files = [fixture_file("one.txt"), fixture_file("two.txt")];
    let transport = restored(clicked(opened("selectOne")));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let error =
        upload_files(&mut connection, &session(), &token(5), &files, deadline()).unwrap_err();
    for file in &files {
        std::fs::remove_file(file).unwrap();
    }

    assert_eq!(error.code, ErrorCode::InvalidArgs);
    assert!(error.message.contains("takes one file, not 2"));
    let sent = sent.lock().unwrap();
    assert_eq!(intercepts(&sent), vec![true, false]);
    assert!(
        !sent
            .iter()
            .any(|request| request["method"] == "DOM.setFileInputFiles")
    );
}

#[test]
fn a_chooser_of_another_tab_is_not_taken_for_this_click() {
    let file = fixture_file("other.txt");
    let mut elsewhere = opened("selectOne");
    elsewhere[0]["sessionId"] = json!("OTHER");
    let transport = restored(clicked(elsewhere));
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

    assert_eq!(error.kind(), Some("no_file_chooser"));
}
