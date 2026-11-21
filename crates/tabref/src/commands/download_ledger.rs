#![cfg_attr(windows, allow(dead_code))]
use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;

use tabref_core::{AppError, read_bounded, write_atomic};
use tabref_page::{DownloadState, now_ms};

use super::download_folder::state_name;
use super::kept_download::KeptDownload;
use crate::session::Session;

const KEPT: usize = 50;
const MAX_BYTES: u64 = 64 * 1024;
const MAX_WATCH_MS: u64 = 24 * 60 * 60 * 1000;

fn expired(entry: &KeptDownload, now: u64) -> bool {
    entry.record.running() && now.saturating_sub(entry.record.timing.started_ms) > MAX_WATCH_MS
}

/// The downloads one browser endpoint's commands have seen, oldest first,
/// each marked with whether a result reported it. Every change runs under
/// a file lock shared by the tabref processes on this state directory, and
/// the file is written only when a change altered it.
#[derive(Debug, Clone)]
pub(crate) struct DownloadLedger {
    path: PathBuf,
    lock: PathBuf,
}

impl DownloadLedger {
    pub(crate) fn pending(&self) -> bool {
        let records = self.read();
        let now = now_ms();
        if records.iter().any(|entry| expired(entry, now)) {
            let _ = self.try_with(|kept| kept.retain(|entry| !expired(entry, now)));
        }
        records.iter().any(|entry| {
            entry.watched
                && !expired(entry, now)
                && (entry.record.running()
                    || entry
                        .cleanup
                        .as_ref()
                        .is_some_and(|cleanup| !cleanup.pending_paths.is_empty())
                    || (!entry.reported
                        && matches!(
                            entry.record.state,
                            DownloadState::Completed | DownloadState::TooLarge { .. }
                        )))
        })
    }

    pub(crate) fn of(session: &Session) -> Self {
        let name = state_name(session, "downloads");
        let base = session.store().base_dir();
        Self {
            path: base.join(&name),
            lock: base.join(format!("{name}.lock")),
        }
    }

    /// Runs `change` on the kept downloads under the lock, waiting for it,
    /// and keeps the newest 50 it leaves.
    pub(crate) fn with<T>(
        &self,
        change: impl FnOnce(&mut Vec<KeptDownload>) -> T,
    ) -> Result<T, AppError> {
        let lock = self.lock_file()?;
        lock.lock()?;
        self.change(change)
    }

    /// [`with`](Self::with) when the lock is free right now; `None` when
    /// another process holds it, so a background chore never blocks.
    pub(crate) fn try_with<T>(
        &self,
        change: impl FnOnce(&mut Vec<KeptDownload>) -> T,
    ) -> Result<Option<T>, AppError> {
        let lock = self.lock_file()?;
        match lock.try_lock() {
            Ok(()) => self.change(change).map(Some),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
    }

    fn lock_file(&self) -> Result<File, AppError> {
        Ok(OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.lock)?)
    }

    fn read(&self) -> Vec<KeptDownload> {
        read_bounded(&self.path, MAX_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn change<T>(&self, change: impl FnOnce(&mut Vec<KeptDownload>) -> T) -> Result<T, AppError> {
        let before = self.read();
        let mut records = before.clone();
        let result = change(&mut records);
        let kept = &records[records.len().saturating_sub(KEPT)..];
        if kept != before.as_slice() {
            if kept.is_empty() {
                match std::fs::remove_file(&self.path) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        return Err(error.into());
                    }
                    _ => {}
                }
            } else {
                write_atomic(&self.path, serde_json::to_string(kept)?.as_bytes())?;
            }
        }
        Ok(result)
    }
}

/// Puts `entry` into `kept`, joining it with the entry of its guid: the
/// newer state of the download unless the kept one already finished, and
/// the marks of both.
pub(crate) fn merge(kept: &mut Vec<KeptDownload>, entry: KeptDownload) {
    let Some(old) = kept
        .iter_mut()
        .find(|old| old.record.guid == entry.record.guid)
    else {
        kept.push(entry);
        return;
    };
    if !matches!(old.record.state, DownloadState::TooLarge { .. })
        && (old.record.running() || !entry.record.running())
    {
        old.record = entry.record;
    }
    old.reported |= entry.reported;
    old.watched |= entry.watched;
    old.batch = entry.batch.or(old.batch);
    if old.cleanup.is_none() {
        old.cleanup = entry.cleanup;
    }
}

#[cfg(test)]
#[path = "download_ledger_tests.rs"]
mod tests;
