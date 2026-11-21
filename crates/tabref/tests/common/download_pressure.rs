use super::{FIRST, MB, enabled, harness, scratch, serve};
use serde_json::{Value, json};

fn seed(tabref: &super::common::Harness, pending: bool) -> std::path::PathBuf {
    let now = tabref_page::now_ms();
    let mut entries: Vec<Value> = (0..49).map(|n| json!({
        "record":{"guid":format!("history-{n}"),"origin":{"url":"u".repeat(256),"suggested_name":"n".repeat(240),"frame_id":"f".repeat(32)},
        "state":if pending {json!({"too_large":{"cap_mb":1}})} else {json!("completed")},"bytes":2*MB,"expected":0,"path":"/outside/", "timing":{"started_ms":now,"finished_ms":now}},
        "reported":true,"watched":true,"batch":null,"cap_bytes":MB,
        "cleanup":if pending {json!({"completed_before_cap":true,"pending_paths":["/outside/"],"removal_verified":false,"failed_tries":1,"first_failed_ms":now,"abandoned_paths":[]})} else {Value::Null}
    })).collect();
    if !pending {
        for entry in &mut entries {
            entry.as_object_mut().unwrap().remove("cleanup");
        }
    }
    while serde_json::to_vec(&entries).unwrap().len() < 65_408 {
        for entry in &mut entries {
            let path = format!("{}p", entry["record"]["path"].as_str().unwrap());
            entry["record"]["path"] = json!(path);
            if pending {
                entry["cleanup"]["pending_paths"][0] = entry["record"]["path"].clone();
            }
        }
    }
    let bytes = serde_json::to_vec(&entries).unwrap();
    assert!(bytes.len() <= 65_536);
    let path = tabref
        .home
        .join(format!("downloads-{}", tabref.browser.port));
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn reported_history_makes_room_for_a_new_watch() {
    check(false);
}

#[test]
fn protected_capacity_failures_are_notes_on_the_action_that_loses_the_new_watch() {
    check(true);
}

fn check(pending: bool) {
    if !enabled() {
        return;
    }
    let (port, _release, _finish) = serve(2 * MB, true);
    let tabref = harness("downloads-pressure");
    let outside = scratch("downloads-pressure");
    let folder = outside.0.join("saved");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        "--download-max-mb",
        "1",
        &format!("http://127.0.0.1:{port}/"),
    ]);
    let path = seed(&tabref, pending);
    let next = tabref.ok(&["--json", "click", "Export"]).json();
    let saved: Vec<Value> = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    if pending {
        assert!(
            next["data"]["download_reconciliation_error"].is_object(),
            "{next}"
        );
        assert_eq!(saved.len(), 49);
        assert!(
            saved
                .iter()
                .all(|entry| entry["cleanup"]["pending_paths"].as_array().unwrap().len() == 1)
        );
    } else {
        assert!(
            saved
                .iter()
                .any(|entry| entry["record"]["state"] == "in_progress"
                    && entry["record"]["bytes"] == FIRST),
            "new watch was silently lost: {next}"
        );
        assert!(saved.len() < 50);
    }
}
