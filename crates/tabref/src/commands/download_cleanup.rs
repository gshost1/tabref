use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tabref_page::{DownloadRecord, DownloadState, now_ms};

use crate::commands::download_place::plain_name;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CapCleanup {
    pub(crate) completed_before_cap: bool,
    pub(crate) pending_paths: Vec<String>,
    #[serde(default)]
    pub(crate) removal_verified: bool,
    #[serde(default)]
    pub(crate) failed_tries: u32,
    #[serde(default)]
    pub(crate) first_failed_ms: Option<u64>,
    #[serde(default)]
    pub(crate) abandoned_paths: Vec<String>,
}

thread_local! {
    static REPORTS: RefCell<BTreeMap<(String, u64), CapCleanup>> = const { RefCell::new(BTreeMap::new()) };
}

pub(crate) fn remember(record: &DownloadRecord, cleanup: Option<CapCleanup>) {
    REPORTS.with_borrow_mut(|reports| {
        let key = (record.guid.clone(), record.timing.started_ms);
        if reports.len() >= 512 && !reports.contains_key(&key) {
            reports.clear();
        }
        if let Some(cleanup) = cleanup {
            reports.insert(key, cleanup);
        } else {
            reports.remove(&key);
        }
    });
}

pub(crate) fn lookup(record: &DownloadRecord) -> Option<CapCleanup> {
    if !matches!(record.state, DownloadState::TooLarge { .. }) {
        return None;
    }
    REPORTS.with_borrow(|reports| {
        reports
            .get(&(record.guid.clone(), record.timing.started_ms))
            .cloned()
    })
}

pub(crate) fn enforce(
    record: &mut DownloadRecord,
    cap: u64,
    folder: Option<&Path>,
    mut cancel: impl FnMut(&str),
) {
    let mut cleanup = match record.state {
        DownloadState::InProgress | DownloadState::Completed if super::past_cap(record, cap) => {
            let completed_before_cap = record.state == DownloadState::Completed;
            if !completed_before_cap {
                cancel(&record.guid);
            }
            let mut paths = Vec::new();
            if let Some((dir, guid)) = folder.zip(plain_name(&record.guid)) {
                let file = record
                    .path
                    .as_deref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| dir.join(guid));
                paths.push(file.to_string_lossy().into_owned());
                paths.push(
                    dir.join(format!("{guid}.crdownload"))
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            record.finish(
                DownloadState::TooLarge {
                    cap_mb: cap / (1024 * 1024),
                },
                now_ms(),
            );
            let removal_verified = !paths.is_empty();
            CapCleanup {
                completed_before_cap,
                pending_paths: paths,
                removal_verified,
                ..CapCleanup::default()
            }
        }
        DownloadState::TooLarge { .. } => match lookup(record) {
            Some(cleanup) => cleanup,
            None => return,
        },
        _ => return,
    };
    cleanup.pending_paths.retain(|path| {
        if folder.is_none_or(|dir| Path::new(path).parent() != Some(dir)) {
            return true;
        }
        match std::fs::remove_file(path) {
            Ok(()) => false,
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        }
    });
    if !cleanup.pending_paths.is_empty() {
        cleanup.failed_tries = cleanup.failed_tries.saturating_add(1);
        let first = *cleanup
            .first_failed_ms
            .get_or_insert_with(|| record.timing.finished_ms.unwrap_or_else(now_ms));
        if cleanup.failed_tries >= 20 || now_ms().saturating_sub(first) >= 24 * 60 * 60 * 1000 {
            cleanup.abandoned_paths = std::mem::take(&mut cleanup.pending_paths);
        }
    }
    record.path = cleanup
        .pending_paths
        .first()
        .or_else(|| cleanup.abandoned_paths.first())
        .cloned();
    remember(record, Some(cleanup));
}

#[cfg(test)]
#[path = "download_cleanup_tests.rs"]
mod tests;
