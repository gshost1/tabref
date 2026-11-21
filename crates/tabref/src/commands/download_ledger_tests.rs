use tabref_page::{DownloadOrigin, DownloadRecord, DownloadState, DownloadTiming};

use super::{DownloadLedger, MAX_BYTES, merge};
use crate::commands::kept_download::KeptDownload;

#[path = "download_ledger_capacity_tests.rs"]
mod capacity;

fn entry(guid: &str, state: DownloadState, reported: bool) -> KeptDownload {
    KeptDownload {
        record: DownloadRecord {
            guid: guid.to_owned(),
            origin: DownloadOrigin::default(),
            state,
            bytes: 1,
            expected: 0,
            path: None,
            timing: DownloadTiming {
                started_ms: tabref_page::now_ms(),
                ..DownloadTiming::default()
            },
        },
        reported,
        watched: false,
        batch: None,
        cleanup: None,
        cap_bytes: None,
    }
}

#[test]
fn a_reported_download_stays_reported_when_another_connection_adds_it_again() {
    let mut kept = vec![entry("g", DownloadState::Completed, true)];
    merge(&mut kept, entry("g", DownloadState::InProgress, false));
    assert_eq!(kept.len(), 1);
    assert!(kept[0].reported);
    assert_eq!(kept[0].record.state, DownloadState::Completed);
    assert!(!kept[0].claimable(None));
}

#[test]
fn only_a_finished_unreported_download_or_one_this_batch_reported_is_claimable() {
    assert!(!entry("a", DownloadState::InProgress, false).claimable(None));
    assert!(entry("b", DownloadState::Completed, false).claimable(None));
    let mut in_batch = entry("c", DownloadState::Completed, true);
    in_batch.batch = Some(7);
    assert!(in_batch.claimable(Some(7)));
    assert!(!in_batch.claimable(Some(8)));
    assert!(!in_batch.claimable(None));
}

#[test]
fn stale_completion_events_do_not_undo_a_cap_failure() {
    let mut kept = vec![entry("g", DownloadState::TooLarge { cap_mb: 1 }, false)];
    merge(&mut kept, entry("g", DownloadState::Completed, false));
    assert_eq!(kept[0].record.state, DownloadState::TooLarge { cap_mb: 1 });
    assert!(kept[0].claimable(None));
}

fn ledger(name: &str) -> DownloadLedger {
    let root = std::env::temp_dir().join(format!("tabref-pending-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    DownloadLedger {
        path: root.join("downloads"),
        lock: root.join("downloads.lock"),
    }
}

#[test]
fn pending_only_selects_watched_running_or_unreported_finished_downloads() {
    let ledger = ledger("states");
    for (state, watched, reported, pending) in [
        (DownloadState::InProgress, false, false, false),
        (DownloadState::InProgress, true, true, true),
        (DownloadState::Completed, true, false, true),
        (DownloadState::Completed, true, true, false),
        (DownloadState::TooLarge { cap_mb: 1 }, true, false, true),
        (DownloadState::TooLarge { cap_mb: 1 }, true, true, false),
        (DownloadState::Canceled, true, false, false),
    ] {
        let mut entry = entry("g", state, reported);
        entry.watched = watched;
        std::fs::write(&ledger.path, serde_json::to_vec(&vec![entry]).unwrap()).unwrap();
        assert_eq!(ledger.pending(), pending, "{state:?}");
        assert!(!ledger.lock.exists());
    }
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn pending_fast_path_is_read_only_for_missing_empty_or_invalid_ledgers() {
    let ledger = ledger("fast");
    let started = std::time::Instant::now();
    for _ in 0..1_000 {
        assert!(!ledger.pending());
    }
    eprintln!(
        "missing ledger: {:.2} us/check (1000 checks)",
        started.elapsed().as_secs_f64() * 1_000.0
    );
    assert!(!ledger.path.exists());
    for content in [
        b"[]".to_vec(),
        b"invalid".to_vec(),
        vec![b' '; MAX_BYTES as usize + 1],
    ] {
        std::fs::write(&ledger.path, &content).unwrap();
        assert_eq!(ledger.pending(), content != b"[]");
        assert_eq!(std::fs::read(&ledger.path).unwrap(), content);
        assert!(!ledger.lock.exists());
    }
    std::fs::write(&ledger.path, b"[]").unwrap();
    let started = std::time::Instant::now();
    for _ in 0..1_000 {
        assert!(!ledger.pending());
    }
    eprintln!(
        "empty ledger: {:.2} us/check (1000 checks)",
        started.elapsed().as_secs_f64() * 1_000.0
    );
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn abandoned_running_watches_expire_without_expiring_pending_removals() {
    let ledger = ledger("expired");
    let mut stale = entry("stale", DownloadState::InProgress, false);
    stale.watched = true;
    stale.record.timing.started_ms = tabref_page::now_ms() - super::MAX_WATCH_MS - 1000;
    std::fs::write(
        &ledger.path,
        serde_json::to_vec(&vec![stale.clone()]).unwrap(),
    )
    .unwrap();
    assert!(!ledger.pending());
    assert!(!ledger.path.exists());
    assert!(!ledger.pending());
    let mut cleanup = stale;
    cleanup.record.state = DownloadState::TooLarge { cap_mb: 1 };
    cleanup.reported = true;
    cleanup.cleanup = Some(crate::commands::downloads::cleanup::CapCleanup {
        completed_before_cap: true,
        pending_paths: vec!["held".into()],
        removal_verified: false,
        ..Default::default()
    });
    std::fs::write(&ledger.path, serde_json::to_vec(&vec![cleanup]).unwrap()).unwrap();
    assert!(ledger.pending());
    assert!(ledger.path.exists());
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn unreadable_or_oversized_ledgers_are_not_overwritten() {
    let ledger = ledger("corrupt");
    for content in [b"invalid".to_vec(), vec![b' '; MAX_BYTES as usize + 1]] {
        std::fs::write(&ledger.path, &content).unwrap();
        assert!(
            ledger
                .with(|kept| kept.push(entry("new", DownloadState::InProgress, false)))
                .is_err()
        );
        assert_eq!(std::fs::read(&ledger.path).unwrap(), content);
        assert!(ledger.pending());
    }
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn page_controlled_fields_are_bounded_without_splitting_utf8() {
    let ledger = ledger("long-fields");
    ledger
        .with(|kept| {
            for n in 0..50 {
                let mut download = entry(&format!("g{n}"), DownloadState::InProgress, false);
                download.record.origin.url = "é".repeat(35_000);
                download.record.origin.suggested_name = "名".repeat(1000);
                merge(kept, download);
            }
        })
        .unwrap();
    assert!(std::fs::metadata(&ledger.path).unwrap().len() <= MAX_BYTES);
    let saved: Vec<KeptDownload> =
        serde_json::from_slice(&std::fs::read(&ledger.path).unwrap()).unwrap();
    assert_eq!(saved.len(), 50);
    assert!(saved[0].record.origin.url.len() <= 256);
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}

#[test]
fn completion_uses_the_latest_running_cap_but_duplicate_events_do_not_change_it() {
    let mut running = entry("cap-change", DownloadState::InProgress, false);
    running.cap_bytes = Some(1000);
    let mut kept = vec![running];
    let mut finished = entry("cap-change", DownloadState::Completed, false);
    finished.cap_bytes = Some(100);
    merge(&mut kept, finished.clone());
    assert_eq!(kept[0].cap_bytes, Some(100));
    finished.cap_bytes = Some(10);
    merge(&mut kept, finished);
    assert_eq!(kept[0].cap_bytes, Some(100));
}

#[test]
fn an_unreadable_ledger_is_not_passed_to_a_mutation_and_manual_cleanup_is_dropped_once_reported() {
    let ledger = ledger("unreadable");
    std::fs::create_dir(&ledger.path).unwrap();
    assert!(ledger.pending());
    assert!(
        ledger
            .with(|_| panic!("must not mutate unreadable data"))
            .is_err()
    );
    std::fs::remove_dir(&ledger.path).unwrap();
    ledger
        .with(|kept| {
            let mut download = entry("manual", DownloadState::TooLarge { cap_mb: 1 }, false);
            download.watched = true;
            download.cleanup = Some(crate::commands::downloads::cleanup::CapCleanup {
                abandoned_paths: vec!["manual-file".into()],
                ..Default::default()
            });
            kept.push(download);
        })
        .unwrap();
    assert!(ledger.pending());
    ledger.with(|kept| kept[0].reported = true).unwrap();
    assert!(!ledger.pending());
    assert!(!ledger.path.exists());
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}
