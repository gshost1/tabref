use serde_json::json;
use tabref_page::{DownloadOrigin, DownloadRecord, DownloadState, DownloadTiming};

use super::{CapCleanup, enforce, lookup, remember};
use crate::commands::download_lines::{download_json, download_line};

fn record(guid: &str) -> DownloadRecord {
    DownloadRecord {
        guid: guid.into(),
        origin: DownloadOrigin {
            suggested_name: "big.bin".into(),
            ..Default::default()
        },
        state: DownloadState::Completed,
        bytes: 2_097_152,
        expected: 0,
        path: None,
        timing: DownloadTiming::default(),
    }
}

#[test]
fn failed_removal_is_persisted_restored_and_retried() {
    let dir = std::env::temp_dir().join(format!("tabref-cleanup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("held")).unwrap();
    let mut record = record("held");
    enforce(&mut record, 1_048_576, Some(&dir), |_| panic!("completed"));
    let pending = lookup(&record).unwrap();
    assert!(pending.completed_before_cap);
    assert_eq!(pending.pending_paths.len(), 1);
    assert!(download_line(&record).contains("delete pending:"));
    assert_eq!(download_json(&record)["removal_pending"], true);
    let bytes = serde_json::to_vec(&pending).unwrap();
    remember(&record, None);
    assert_eq!(lookup(&record), None);
    let restored: CapCleanup = serde_json::from_slice(&bytes).unwrap();
    remember(&record, Some(restored));
    std::fs::remove_dir(dir.join("held")).unwrap();
    std::fs::write(dir.join("held"), b"finished").unwrap();
    enforce(&mut record, 1_048_576, Some(&dir), |_| {
        panic!("already capped")
    });
    assert!(lookup(&record).unwrap().pending_paths.is_empty());
    assert!(record.path.is_none());
    assert!(download_line(&record).contains("removed"));
    assert_eq!(download_json(&record)["removal_pending"], false);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn completed_files_are_removed_and_running_downloads_are_canceled() {
    for completed in [false, true] {
        let mut record = record(if completed { "complete" } else { "running" });
        if !completed {
            record.state = DownloadState::InProgress;
        }
        let mut canceled = false;
        enforce(&mut record, 1_048_576, None, |_| canceled = true);
        assert_eq!(canceled, !completed);
        assert!(download_line(&record).contains(if completed {
            "removal unverified"
        } else {
            "canceled"
        }));
        assert_eq!(
            download_json(&record)["completed_before_cap"],
            json!(completed)
        );
        assert_eq!(record.path, None);
    }
}
