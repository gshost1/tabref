use super::{entry, ledger};
use tabref_page::DownloadState;

#[test]
fn oversized_valid_protected_records_are_never_quarantined_or_overwritten() {
    let ledger = ledger("capacity-protected");
    let entries: Vec<_> = (0..60)
        .map(|n| {
            let mut pending = entry(
                &format!("pending-{n}"),
                DownloadState::TooLarge { cap_mb: 1 },
                true,
            );
            pending.record.origin.url = "u".repeat(256);
            pending.watched = true;
            pending.cleanup = Some(crate::commands::downloads::cleanup::CapCleanup {
                pending_paths: vec![format!("/held/{}", "p".repeat(900))],
                ..Default::default()
            });
            pending
        })
        .collect();
    let bytes = serde_json::to_vec(&entries).unwrap();
    assert!(bytes.len() > super::super::MAX_BYTES as usize);
    std::fs::write(&ledger.path, &bytes).unwrap();
    assert_eq!(
        ledger.recovering_with(|_| {}).unwrap_err().kind(),
        Some("download_ledger_full")
    );
    assert_eq!(std::fs::read(&ledger.path).unwrap(), bytes);
    assert!(
        !std::fs::read_dir(ledger.path.parent().unwrap())
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-"))
    );
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn recovery_is_bounded_and_does_not_label_an_unparsed_file_corrupt() {
    let ledger = ledger("capacity-unparsed");
    let bytes = vec![b' '; super::super::RECOVERY_BYTES as usize + 1];
    std::fs::write(&ledger.path, &bytes).unwrap();
    assert_eq!(
        ledger.recovering_with(|_| {}).unwrap_err().kind(),
        Some("download_ledger_unreadable")
    );
    assert_eq!(std::fs::read(&ledger.path).unwrap(), bytes);
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn malformed_ledgers_are_saved_exactly_once_and_fresh_watches_are_kept() {
    let ledger = ledger("capacity-recovery");
    std::fs::write(&ledger.path, b"{broken").unwrap();
    ledger
        .recovering_with(|kept| kept.push(entry("new-watch", DownloadState::InProgress, false)))
        .unwrap();
    ledger.recovering_with(|_| {}).unwrap();
    assert_eq!(ledger.read().unwrap()[0].record.guid, "new-watch");
    let backups: Vec<_> = std::fs::read_dir(ledger.path.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("downloads-ledger.corrupt-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(backups[0].path()).unwrap(), b"{broken");
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn whitespace_only_oversizing_is_compacted_even_without_a_record_change() {
    let ledger = ledger("capacity-whitespace");
    let mut bytes = serde_json::to_vec(&vec![entry("g", DownloadState::Completed, true)]).unwrap();
    bytes.resize(super::super::MAX_BYTES as usize + 1, b' ');
    std::fs::write(&ledger.path, bytes).unwrap();
    ledger.with(|_| {}).unwrap();
    assert!(std::fs::metadata(&ledger.path).unwrap().len() <= super::super::MAX_BYTES);
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn valid_oversized_history_is_compacted_not_quarantined() {
    let ledger = ledger("capacity-legacy");
    let entries: Vec<_> = (0..60)
        .map(|n| {
            let mut old = entry(&format!("legacy-{n}"), DownloadState::Completed, true);
            old.record.origin.url = "u".repeat(1500);
            old
        })
        .collect();
    let bytes = serde_json::to_vec(&entries).unwrap();
    assert!(bytes.len() > super::super::MAX_BYTES as usize);
    std::fs::write(&ledger.path, bytes).unwrap();
    ledger.with(|_| {}).unwrap();
    assert!(std::fs::metadata(&ledger.path).unwrap().len() <= super::super::MAX_BYTES);
    assert_eq!(ledger.read().unwrap().len(), 50);
    assert!(
        !std::fs::read_dir(ledger.path.parent().unwrap())
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-"))
    );
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn byte_pressure_evicts_reported_history_not_running_watches() {
    let ledger = ledger("capacity-history");
    ledger
        .with(|kept| {
            let mut running = entry("old-watch", DownloadState::InProgress, false);
            running.watched = true;
            kept.push(running);
            for n in 0..50 {
                let mut old = entry(&format!("old-{n}"), DownloadState::Completed, true);
                old.record.origin.url = "u".repeat(256);
                old.record.origin.suggested_name = "n".repeat(240);
                old.record.origin.frame_id = "f".repeat(256);
                old.record.path = Some(format!("/history/{}", "p".repeat(700)));
                kept.push(old);
            }
            kept.push(entry("new-watch", DownloadState::InProgress, false));
        })
        .unwrap();
    let saved = ledger.read().unwrap();
    assert!(saved.iter().any(|entry| entry.record.guid == "old-watch"));
    assert!(saved.iter().any(|entry| entry.record.guid == "new-watch"));
    assert!(!saved.iter().any(|entry| entry.record.guid == "old-0"));
    assert!(std::fs::metadata(&ledger.path).unwrap().len() <= super::super::MAX_BYTES);
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn the_history_count_never_evicts_protected_watches() {
    let ledger = ledger("capacity-count");
    ledger
        .with(|kept| {
            for n in 0..60 {
                let mut running = entry(&format!("watch-{n}"), DownloadState::InProgress, false);
                running.watched = true;
                kept.push(running);
            }
        })
        .unwrap();
    assert_eq!(ledger.read().unwrap().len(), 60);
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn actionable_paths_are_never_truncated_to_fit() {
    let ledger = ledger("capacity-path");
    ledger
        .with(|kept| kept.push(entry("original", DownloadState::InProgress, false)))
        .unwrap();
    let before = std::fs::read(&ledger.path).unwrap();
    let result = ledger.with(|kept| {
        let mut pending = entry("pending", DownloadState::TooLarge { cap_mb: 1 }, true);
        pending.watched = true;
        let path = format!("/held/{}", "p".repeat(2000));
        pending.record.path = Some(path.clone());
        pending.cleanup = Some(crate::commands::downloads::cleanup::CapCleanup {
            pending_paths: vec![path],
            ..Default::default()
        });
        kept.push(pending);
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&ledger.path).unwrap(), before);
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}
