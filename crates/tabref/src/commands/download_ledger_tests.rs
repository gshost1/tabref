use tabref_page::{DownloadOrigin, DownloadRecord, DownloadState, DownloadTiming};

use super::merge;
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
            timing: DownloadTiming::default(),
        },
        reported,
        watched: false,
        batch: None,
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
