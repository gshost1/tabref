use serde_json::json;

use super::apply_event;
use crate::CdpEvent;
use crate::settle::download_state::DownloadState;

fn event(method: &str, params: serde_json::Value) -> CdpEvent {
    CdpEvent {
        method: method.to_owned(),
        params,
        session_id: None,
    }
}

#[test]
fn a_download_is_recorded_once_and_followed_to_completion() {
    let mut records = Vec::new();
    let begin = event(
        "Browser.downloadWillBegin",
        json!({ "guid": "g1", "url": "http://a.test/r.csv", "suggestedFilename": "r.csv", "frameId": "F1" }),
    );
    apply_event(&mut records, &begin, 1_000);
    apply_event(&mut records, &begin, 1_100);
    apply_event(
        &mut records,
        &event(
            "Browser.downloadProgress",
            json!({ "guid": "g1", "receivedBytes": 12, "state": "inProgress" }),
        ),
        1_200,
    );
    apply_event(
        &mut records,
        &event(
            "Browser.downloadProgress",
            json!({ "guid": "g1", "receivedBytes": 24, "state": "completed", "filePath": "/d/g1" }),
        ),
        1_340,
    );
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.origin.url, "http://a.test/r.csv");
    assert_eq!(record.origin.suggested_name, "r.csv");
    assert_eq!(record.state, DownloadState::Completed);
    assert_eq!(record.bytes, 24);
    assert_eq!(record.path.as_deref(), Some("/d/g1"));
    assert_eq!(record.timing.elapsed_ms(), Some(340));
    assert_eq!(record.origin.frame_id, "F1");
}

#[test]
fn a_finished_download_is_not_reopened_by_a_late_event() {
    let mut records = Vec::new();
    apply_event(
        &mut records,
        &event("Browser.downloadWillBegin", json!({ "guid": "g1" })),
        0,
    );
    apply_event(
        &mut records,
        &event(
            "Browser.downloadProgress",
            json!({ "guid": "g1", "state": "canceled" }),
        ),
        5,
    );
    apply_event(
        &mut records,
        &event(
            "Browser.downloadProgress",
            json!({ "guid": "g1", "state": "inProgress", "receivedBytes": 9 }),
        ),
        6,
    );
    assert_eq!(records[0].state, DownloadState::Canceled);
    assert_eq!(records[0].bytes, 0);
}
