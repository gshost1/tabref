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
const RECOVERY_BYTES: u64 = 1024 * 1024;
const MAX_WATCH_MS: u64 = 24 * 60 * 60 * 1000;

#[path = "download_ledger_fit.rs"]
mod fit;
#[path = "download_ledger_quarantine.rs"]
mod quarantine;

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
        let Ok(records) = self.read() else {
            return true;
        };
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
    /// retaining protected downloads and up to 50 history entries.
    pub(crate) fn with<T>(
        &self,
        change: impl FnOnce(&mut Vec<KeptDownload>) -> T,
    ) -> Result<T, AppError> {
        let lock = self.lock_file()?;
        lock.lock()?;
        self.change(change, false)
    }

    pub(crate) fn recovering_with<T>(
        &self,
        change: impl FnOnce(&mut Vec<KeptDownload>) -> T,
    ) -> Result<T, AppError> {
        let lock = self.lock_file()?;
        lock.lock()?;
        self.change(change, true)
    }

    /// [`with`](Self::with) when the lock is free right now; `None` when
    /// another process holds it, so a background chore never blocks.
    pub(crate) fn try_with<T>(
        &self,
        change: impl FnOnce(&mut Vec<KeptDownload>) -> T,
    ) -> Result<Option<T>, AppError> {
        let lock = self.lock_file()?;
        match lock.try_lock() {
            Ok(()) => self.change(change, false).map(Some),
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

    fn read(&self) -> Result<Vec<KeptDownload>, AppError> {
        self.read_sized().map(|(records, _)| records)
    }

    fn read_sized(&self) -> Result<(Vec<KeptDownload>, bool), AppError> {
        let bytes = match read_bounded(&self.path, RECOVERY_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok((Vec::new(), false));
            }
            Err(error) => return Err(self.unreadable(error)),
        };
        let records = serde_json::from_slice(&bytes).map_err(|error| {
            self.unreadable(error).with_details(
                serde_json::json!({"kind":"download_ledger_corrupt","path":self.path}),
            )
        })?;
        Ok((records, bytes.len() as u64 > MAX_BYTES))
    }

    fn unreadable(&self, error: impl std::fmt::Display) -> AppError {
        AppError::invalid_args(format!("Cannot read download ledger {}: {error}; repair it or move it aside before retrying download tracking", self.path.display()))
            .with_details(serde_json::json!({"kind":"download_ledger_unreadable", "path":self.path}))
    }

    fn change<T>(
        &self,
        change: impl FnOnce(&mut Vec<KeptDownload>) -> T,
        recover: bool,
    ) -> Result<T, AppError> {
        let (before, oversized) = match self.read_sized() {
            Err(error) if recover && error.kind() == Some("download_ledger_corrupt") => {
                let backup =
                    quarantine::move_aside(&self.path).map_err(|error| self.unreadable(error))?;
                super::downloads::note_error(
                    AppError::invalid_args(format!(
                        "Moved malformed download ledger to {}; tracking restarted",
                        backup.display()
                    ))
                    .with_details(
                        serde_json::json!({"kind":"download_ledger_quarantined","backup":backup}),
                    ),
                );
                (Vec::new(), false)
            }
            result => result?,
        };
        let mut records = before.clone();
        let result = change(&mut records);
        records.retain(|entry| {
            !entry.reported
                || entry
                    .cleanup
                    .as_ref()
                    .is_none_or(|cleanup| cleanup.abandoned_paths.is_empty())
        });
        let bytes = fit::encode(&mut records)?;
        let kept = records.as_slice();
        if kept != before.as_slice() || oversized {
            if kept.is_empty() {
                match std::fs::remove_file(&self.path) {
                    Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                        return Err(error.into());
                    }
                    _ => {}
                }
            } else {
                write_atomic(&self.path, &bytes)?;
            }
        }
        Ok(result)
    }
}

fn bound_fields(entry: &mut KeptDownload) {
    for field in [
        &mut entry.record.origin.url,
        &mut entry.record.origin.suggested_name,
        &mut entry.record.origin.frame_id,
    ] {
        let mut size = 0;
        let end = field.char_indices().find_map(|(index, ch)| {
            size += match ch {
                '"' | '\\' => 2,
                '\u{0}'..='\u{1f}' => 6,
                _ => ch.len_utf8(),
            };
            (size > 256).then_some(index)
        });
        if let Some(end) = end {
            field.truncate(end);
        }
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
    if old.record.running() {
        old.cap_bytes = entry.cap_bytes.or(old.cap_bytes);
    }
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
