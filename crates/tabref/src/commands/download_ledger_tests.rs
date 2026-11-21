use tabref_page::{DownloadOrigin, DownloadRecord, DownloadState, DownloadTiming};

use super::{DownloadLedger, MAX_BYTES, merge};
use crate::commands::kept_download::KeptDownload;

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
        assert!(!ledger.pending());
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
    });
    std::fs::write(&ledger.path, serde_json::to_vec(&vec![cleanup]).unwrap()).unwrap();
    assert!(ledger.pending());
    assert!(ledger.path.exists());
    std::fs::remove_dir_all(ledger.path.parent().unwrap()).unwrap();
}
