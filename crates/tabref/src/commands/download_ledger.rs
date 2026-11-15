use std::fs::{File, OpenOptions, TryLockError};
use std::path::PathBuf;

use tabref_core::{AppError, read_bounded, write_atomic};

use super::download_folder::state_name;
use super::kept_download::KeptDownload;
use crate::session::Session;

const KEPT: usize = 50;
const MAX_BYTES: u64 = 64 * 1024;

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

    fn change<T>(&self, change: impl FnOnce(&mut Vec<KeptDownload>) -> T) -> Result<T, AppError> {
        let before: Vec<KeptDownload> = read_bounded(&self.path, MAX_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let mut records = before.clone();
        let result = change(&mut records);
        let kept = &records[records.len().saturating_sub(KEPT)..];
        if kept != before.as_slice() {
            write_atomic(&self.path, serde_json::to_string(kept)?.as_bytes())?;
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
    if old.record.running() || !entry.record.running() {
        old.record = entry.record;
    }
    old.reported |= entry.reported;
    old.watched |= entry.watched;
    old.batch = entry.batch.or(old.batch);
}

#[cfg(test)]
#[path = "download_ledger_tests.rs"]
mod tests;
