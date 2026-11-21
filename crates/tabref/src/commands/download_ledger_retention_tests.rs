use super::{entry, ledger};
use crate::commands::downloads::{cleanup::CapCleanup, refresh_kept};
use tabref_page::DownloadState;

struct NoRequests;

impl tabref_cdp::CdpTransport for NoRequests {
    fn send_text(
        &mut self,
        _: &str,
        _: tabref_core::Deadline,
    ) -> Result<(), tabref_core::AppError> {
        panic!("finished file needs no CDP request")
    }

    fn receive_text(&mut self, _: tabref_core::Deadline) -> Result<String, tabref_core::AppError> {
        panic!("finished file needs no CDP reply")
    }
}

#[test]
fn unreported_finished_history_keeps_the_newest_fifty_and_a_new_watch_is_capped() {
    let ledger = ledger("unreported-history");
    let mut records: Vec<_> = (0..120)
        .map(|n| {
            let mut old = entry(&format!("old-{n}"), DownloadState::Completed, false);
            old.record.origin.url = "u".repeat(256);
            old.record.origin.suggested_name = "n".repeat(240);
            old.record.origin.frame_id = "f".repeat(256);
            old
        })
        .collect();
    let mut running = entry("old-running", DownloadState::InProgress, false);
    running.watched = true;
    records.insert(0, running);
    let mut pending = entry("old-removal", DownloadState::TooLarge { cap_mb: 1 }, false);
    pending.cleanup = Some(CapCleanup {
        pending_paths: vec!["held".into()],
        ..Default::default()
    });
    records.insert(0, pending);
    let bytes = serde_json::to_vec(&records).unwrap();
    assert!(bytes.len() > super::super::MAX_BYTES as usize);
    std::fs::write(&ledger.path, bytes).unwrap();
    ledger
        .with(|kept| {
            let mut new = entry("new-over-cap", DownloadState::InProgress, false);
            new.watched = true;
            new.cap_bytes = Some(100);
            kept.push(new);
        })
        .unwrap();
    let saved = ledger.read().unwrap();
    assert_eq!(saved.len(), 53);
    assert_eq!(saved[0].record.guid, "old-removal");
    assert_eq!(saved[1].record.guid, "old-running");
    for (n, old) in saved[2..52].iter().enumerate() {
        assert_eq!(old.record.guid, format!("old-{}", n + 70));
        assert!(!old.reported);
    }
    assert_eq!(saved[52].record.guid, "new-over-cap");
    assert!(saved[52].watched);
    let folder = ledger.path.parent().unwrap().join("saved");
    std::fs::create_dir(&folder).unwrap();
    let file = folder.join("new-over-cap");
    std::fs::write(&file, [0; 101]).unwrap();
    ledger
        .with(|kept| {
            let new = kept.last_mut().unwrap();
            let mut connection = tabref_cdp::CdpConnection::new(Box::new(NoRequests));
            refresh_kept(
                &mut connection,
                std::slice::from_mut(new),
                100,
                Some(&folder),
                tabref_core::Deadline::after(1000).unwrap(),
            );
            assert!(matches!(new.record.state, DownloadState::TooLarge { .. }));
        })
        .unwrap();
    assert!(!file.exists());
    assert!(matches!(
        ledger.read().unwrap().last().unwrap().record.state,
        DownloadState::TooLarge { .. }
    ));
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn corrupt_backups_keep_only_the_newest_three_without_touching_other_files() {
    let ledger = ledger("backup-retention");
    let root = ledger.path.parent().unwrap();
    let unrelated = root.join("downloads-ledger.corrupt-unrelated.json");
    std::fs::write(&unrelated, b"keep").unwrap();
    let directory = root.join("downloads-ledger.corrupt-1-1-1.json");
    std::fs::create_dir(&directory).unwrap();
    let mut backups = Vec::new();
    for n in 0..8 {
        let content = format!("broken-{n}");
        std::fs::write(&ledger.path, &content).unwrap();
        backups.push(super::super::quarantine::move_aside(&ledger.path).unwrap());
        assert!(!ledger.path.exists());
        for (index, backup) in backups.iter().enumerate() {
            if index + 3 > n {
                assert_eq!(
                    std::fs::read_to_string(backup).unwrap(),
                    format!("broken-{index}")
                );
            } else {
                assert!(!backup.exists(), "{}", backup.display());
            }
        }
    }
    assert_eq!(std::fs::read(&unrelated).unwrap(), b"keep");
    assert!(directory.is_dir());
    assert_eq!(std::fs::read_dir(root).unwrap().count(), 5);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_missing_ledger_does_not_prune_backups() {
    let ledger = ledger("backup-missing");
    let root = ledger.path.parent().unwrap();
    let backup = root.join("downloads-ledger.corrupt-1-1-1.json");
    std::fs::write(&backup, b"preserved").unwrap();
    assert!(super::super::quarantine::move_aside(&ledger.path).is_err());
    assert_eq!(std::fs::read(&backup).unwrap(), b"preserved");
    std::fs::remove_dir_all(root).unwrap();
}
