use tabref_page::{DownloadOrigin, DownloadRecord, DownloadState, DownloadTiming};

use super::past_cap;
use crate::commands::download_ledger::merge;
use crate::commands::download_lines::{download_line, file_name};

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
fn a_kept_download_is_replaced_by_its_guid() {
    let mut kept = vec![record("a", 1, 0), record("b", 1, 0)];
    let mut newer = record("a", 9, 0);
    newer.state = DownloadState::Completed;
    merge(&mut kept, newer);
    merge(&mut kept, record("c", 1, 0));
    assert_eq!(kept.len(), 3);
    assert_eq!(kept[0].bytes, 9);
    assert_eq!(kept[2].guid, "c");
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
