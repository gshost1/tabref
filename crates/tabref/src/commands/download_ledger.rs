use std::fs::OpenOptions;
use std::path::PathBuf;

use tabref_core::{AppError, read_bounded, write_atomic};
use tabref_page::DownloadRecord;

use super::download_folder::state_name;
use crate::session::Session;

const KEPT: usize = 50;
const MAX_BYTES: u64 = 64 * 1024;

/// The downloads of one browser endpoint that no command has reported
/// finished yet, oldest first: those still running, and those that
/// finished while no command watched. Every change runs under a file lock
/// shared by the tabref processes on this state directory.
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

    /// Runs `change` on the kept downloads under the lock and keeps the
    /// newest 50 it leaves.
    pub(crate) fn with<T>(
        &self,
        change: impl FnOnce(&mut Vec<DownloadRecord>) -> T,
    ) -> Result<T, AppError> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.lock)?;
        lock.lock()?;
        let mut records: Vec<DownloadRecord> = read_bounded(&self.path, MAX_BYTES)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let result = change(&mut records);
        let kept = &records[records.len().saturating_sub(KEPT)..];
        write_atomic(&self.path, serde_json::to_string(kept)?.as_bytes())?;
        Ok(result)
    }
}

/// Puts `record` into `records`, replacing the one with its guid.
pub(crate) fn merge(records: &mut Vec<DownloadRecord>, record: DownloadRecord) {
    match records.iter_mut().find(|kept| kept.guid == record.guid) {
        Some(kept) => *kept = record,
        None => records.push(record),
    }
}
