use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::json;
use tabref_core::{AppError, Deadline, ErrorCode};

use super::CdpConnection;
use crate::ScriptedTransport;

fn deadline() -> Deadline {
    Deadline::after(1_000).unwrap()
}

#[test]
fn a_restoring_call_is_sent_for_a_cancelled_caller_and_the_cancel_stays() {
    let transport =
        ScriptedTransport::new().expect("Page.setInterceptFileChooserDialog", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    connection.set_cancel(Some(Arc::new(AtomicBool::new(true))));

    let refused = connection
        .call("DOM.focus", json!({}), Some("S"), deadline())
        .unwrap_err();
    assert_eq!(refused.kind(), Some("cancelled"));
    connection
        .call_restoring(
            "Page.setInterceptFileChooserDialog",
            json!({ "enabled": false }),
            Some("S"),
            deadline(),
        )
        .unwrap();
    let again = connection
        .call("DOM.focus", json!({}), Some("S"), deadline())
        .unwrap_err();
    assert_eq!(again.kind(), Some("cancelled"));
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[test]
fn a_restoring_call_still_obeys_the_guard() {
    let mut connection = CdpConnection::new(ScriptedTransport::new().boxed());
    connection.set_guard(Some(Arc::new(|| {
        Err(AppError::new(ErrorCode::PermDenied, "opt-in withdrawn"))
    })));
    let error = connection
        .call_restoring(
            "Page.setInterceptFileChooserDialog",
            json!({ "enabled": false }),
            Some("S"),
            deadline(),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::PermDenied);
}
