use tabref_page::{DownloadOrigin, DownloadRecord, DownloadState, DownloadTiming};

use super::{past_cap, refresh_record};
use crate::commands::download_lines::{download_line, file_name};

struct NoRequests;
impl tabref_cdp::CdpTransport for NoRequests {
    fn send_text(
        &mut self,
        _: &str,
        _: tabref_core::Deadline,
    ) -> Result<(), tabref_core::AppError> {
        panic!("unexpected request")
    }
    fn receive_text(&mut self, _: tabref_core::Deadline) -> Result<String, tabref_core::AppError> {
        panic!("unexpected receive")
    }
}

fn record(guid: &str, bytes: u64, expected: u64) -> DownloadRecord {
    DownloadRecord {
        guid: guid.to_owned(),
        origin: DownloadOrigin {
            url: "http://a.test/big.bin".into(),
            suggested_name: "big.bin".into(),
            frame_id: "F".into(),
        },
        state: DownloadState::InProgress,
        bytes,
        expected,
        path: None,
        timing: DownloadTiming::default(),
    }
}

#[test]
fn a_download_passes_the_cap_by_what_it_received_or_announced() {
    assert!(!past_cap(&record("g", 10, 0), 100));
    assert!(past_cap(&record("g", 101, 0), 100));
    assert!(past_cap(&record("g", 10, 2_000), 100));
}

#[test]
fn lowering_the_cap_does_not_remove_an_unclaimed_completed_download() {
    let dir = folder("lowered-cap");
    let path = dir.join("big.bin");
    std::fs::write(&path, [0; 500]).unwrap();
    let mut download = record("lower-cap", 500, 0);
    download.state = DownloadState::Completed;
    download.path = Some(path.to_string_lossy().into_owned());
    let mut kept = vec![crate::commands::kept_download::KeptDownload {
        record: download,
        reported: false,
        watched: true,
        batch: None,
        cleanup: None,
        cap_bytes: Some(1000),
    }];
    let mut connection = tabref_cdp::CdpConnection::new(Box::new(NoRequests));
    let deadline = tabref_core::Deadline::after(1000).unwrap();
    super::refresh_kept(&mut connection, &mut kept, 1000, Some(&dir), deadline);
    super::refresh_kept(&mut connection, &mut kept, 100, Some(&dir), deadline);
    assert!(path.exists());
    assert_eq!(kept[0].record.state, DownloadState::Completed);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_guid_cannot_remove_a_partial_file_outside_the_download_folder() {
    let root = folder("guid");
    let dir = root.join("downloads");
    std::fs::create_dir(&dir).unwrap();
    let outside = root.join("victim.crdownload");
    std::fs::write(&outside, [0; 101]).unwrap();
    for guid in ["../victim", "..\\victim", "/victim", ""] {
        let mut download = record(guid, 101, 0);
        refresh_record(&mut download, true, 100, Some(&dir), |_| {});
        assert!(outside.exists());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_download_past_the_cap_says_so_and_how_to_raise_it() {
    let mut capped = record("g", 2_097_152, 0);
    capped.state = DownloadState::TooLarge { cap_mb: 1 };
    assert_eq!(file_name(&capped), "big.bin");
    assert_eq!(
        download_line(&capped),
        "download \"big.bin\" canceled at 2097152 bytes: over the 1 MB download cap; raise it with open --download-max-mb"
    );
}

fn folder(name: &str) -> std::path::PathBuf {
    let root = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("target"));
    let dir = root.join(format!("download-reconcile-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn disk_size_is_checked_before_a_missed_completion_is_named() {
    let dir = folder("over-cap");
    std::fs::write(dir.join("ab12"), [0; 101]).unwrap();
    let mut download = record("ab12", 1, 0);
    refresh_record(&mut download, true, 100, Some(&dir), |_| {
        panic!("already finished")
    });
    assert_eq!(download.bytes, 101);
    assert!(matches!(download.state, DownloadState::TooLarge { .. }));
    assert_eq!(download.path, None);
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_partial_file_is_canceled_using_its_current_disk_size() {
    let dir = folder("partial");
    std::fs::write(dir.join("ab12.crdownload"), [0; 101]).unwrap();
    let mut download = record("ab12", 1, 0);
    let mut canceled = Vec::new();
    refresh_record(&mut download, true, 100, Some(&dir), |guid| {
        canceled.push(guid.to_owned())
    });
    assert_eq!(canceled, ["ab12"]);
    assert_eq!(download.bytes, 101);
    assert!(matches!(download.state, DownloadState::TooLarge { .. }));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn completed_files_at_the_cap_are_named_and_unwatched_files_are_not_capped() {
    for (name, watched, bytes) in [("at-cap", true, 100), ("unwatched", false, 101)] {
        let dir = folder(name);
        std::fs::write(dir.join("ab12"), vec![0; bytes]).unwrap();
        let mut download = record("ab12", 1, 0);
        refresh_record(&mut download, watched, 100, Some(&dir), |_| {
            panic!("not capped")
        });
        assert_eq!(download.bytes, bytes as u64);
        assert_eq!(download.state, DownloadState::Completed);
        assert_eq!(download.path.as_deref(), dir.join("big.bin").to_str());
        assert_eq!(
            std::fs::metadata(dir.join("big.bin")).unwrap().len(),
            bytes as u64
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn missing_files_do_not_finish_a_running_download() {
    let mut download = record("ab12", 1, 0);
    refresh_record(&mut download, true, 100, None, |_| panic!("not capped"));
    assert_eq!(download.state, DownloadState::InProgress);
    assert_eq!(download.path, None);
}

#[test]
fn completed_event_bytes_are_refreshed_from_disk_before_naming() {
    let dir = folder("completed-event");
    std::fs::write(dir.join("ab12"), [0; 101]).unwrap();
    let mut download = record("ab12", 1, 0);
    download.state = DownloadState::Completed;
    refresh_record(&mut download, true, 100, Some(&dir), |_| {
        panic!("already finished")
    });
    assert_eq!(download.bytes, 101);
    assert!(matches!(download.state, DownloadState::TooLarge { .. }));
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pending_reports_append_to_the_existing_download_json_shape() {
    use crate::output::outcome::Outcome;
    use serde_json::json;

    let mut outcome = Outcome::new(
        "clicked",
        json!({ "action": "click", "downloads": [{ "name": "first.bin" }] }),
    );
    let before = outcome.clone();
    outcome.extend(Vec::<DownloadRecord>::new());
    assert_eq!(outcome, before);
    let mut capped = record("ab12", 101, 0);
    capped.state = DownloadState::TooLarge { cap_mb: 1 };
    outcome.extend([capped.clone()]);
    assert_eq!(outcome.text, format!("clicked\n{}", download_line(&capped)));
    assert_eq!(outcome.data["action"], "click");
    assert_eq!(outcome.data["downloads"][0], before.data["downloads"][0]);
    assert_eq!(
        outcome.data["downloads"][1],
        crate::commands::download_lines::download_json(&capped)
    );
}
