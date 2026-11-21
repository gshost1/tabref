#![cfg_attr(windows, allow(dead_code))]
//! The daemon's download chores outside commands, for a browser tabref
//! launched only: folding the events its idle connections got, so every
//! download that lands is named, and canceling what tabref started when
//! the daemon retires. A browser tabref did not launch has its events off
//! between commands and is never touched here.

use tabref_core::Deadline;
use tabref_page::{DownloadRecord, DownloadState, cancel_download, note_downloads, now_ms};

use super::download_folder::{download_cap, download_folder, launched_here};
use super::download_ledger::{DownloadLedger, merge};
use super::downloads::refresh_kept;
use super::kept_download::KeptDownload;
use crate::session::Session;

const TENDING: u64 = 300;

/// Folds the download events this idle connection got into the kept list
/// and names every download that landed. Only downloads a tabref action or
/// wait saw begin are capped; one first seen here is named, never stopped.
/// Skipped while another process holds the list.
pub(crate) fn tend(session: &mut Session) {
    if !launched_here(session) {
        return;
    }
    let Ok(deadline) = Deadline::after(TENDING) else {
        return;
    };
    let folder = download_folder(session).ok().flatten();
    let cap = download_cap(session);
    let ledger = DownloadLedger::of(session);
    let _ = ledger.try_with(|kept| {
        let mut records: Vec<DownloadRecord> =
            kept.iter().map(|entry| entry.record.clone()).collect();
        let _ = note_downloads(session.connection(), &mut records, deadline);
        for record in records {
            let entry = KeptDownload {
                record,
                reported: false,
                watched: false,
                batch: None,
                cleanup: None,
                cap_bytes: Some(cap),
            };
            merge(kept, entry);
        }
        refresh_kept(session.connection(), kept, cap, folder.as_deref(), deadline);
    });
}

/// When the daemon retires: cancels the downloads still running that a
/// tabref action or wait saw begin in a browser tabref launched, as no
/// process will be left to name them.
pub(crate) fn retire(session: &mut Session) {
    if !launched_here(session) {
        return;
    }
    let Ok(deadline) = Deadline::after(TENDING) else {
        return;
    };
    let ledger = DownloadLedger::of(session);
    let _ = ledger.with(|kept| {
        for entry in kept
            .iter_mut()
            .filter(|entry| entry.watched && entry.record.running())
        {
            let _ = cancel_download(session.connection(), &entry.record.guid, deadline);
            entry.record.finish(DownloadState::Canceled, now_ms());
        }
    });
}
