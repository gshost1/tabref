//! `wait --download`: the oldest download no result reported finished, or
//! one a step of the same batch reported.

use std::time::Duration;

use serde_json::json;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};
use tabref_page::{DownloadRecord, DownloadState, note_downloads};

use super::download_folder::{download_cap, download_folder};
use super::download_ledger::{DownloadLedger, merge};
use super::download_lines::{download_json, download_line};
use super::downloads::{current_batch, enable, enforce_cap, land_from_disk, name_landed};
use super::kept_download::KeptDownload;
use crate::session::Session;

const WAIT_POLL: Duration = Duration::from_millis(50);

/// Waits within `deadline` for the oldest download no result has reported
/// finished, and returns it, marked reported: one an earlier command left
/// running (found in the folder when its events were missed), one that
/// finished between commands, one that starts during the wait, or, in a
/// batch, one an earlier step of the batch reported.
pub(crate) fn wait_download(
    session: &mut Session,
    deadline: Deadline,
) -> Result<DownloadRecord, AppError> {
    enable(session, deadline)?;
    let folder = download_folder(session);
    let cap = download_cap(session);
    let batch = current_batch();
    let ledger = DownloadLedger::of(session);
    loop {
        let done = ledger.with(|kept| {
            let mut records: Vec<DownloadRecord> =
                kept.iter().map(|entry| entry.record.clone()).collect();
            let _ = note_downloads(session.connection(), &mut records, deadline);
            for record in records {
                let began_here = kept.iter().all(|old| old.record.guid != record.guid);
                let entry = KeptDownload {
                    record,
                    reported: false,
                    watched: began_here,
                    batch: None,
                };
                merge(kept, entry);
            }
            let mut watched: Vec<DownloadRecord> = kept
                .iter()
                .filter(|entry| entry.watched)
                .map(|entry| entry.record.clone())
                .collect();
            enforce_cap(
                session.connection(),
                &mut watched,
                cap,
                folder.as_deref(),
                deadline,
            );
            for record in watched {
                merge(
                    kept,
                    KeptDownload {
                        record,
                        reported: false,
                        watched: true,
                        batch: None,
                    },
                );
            }
            for entry in kept.iter_mut() {
                land_from_disk(&mut entry.record, folder.as_deref());
                name_landed(&mut entry.record, folder.as_deref());
            }
            match kept.iter_mut().find(|entry| entry.claimable(batch)) {
                Some(entry) => {
                    entry.reported = true;
                    entry.batch = None;
                    Ok(entry.record.clone())
                }
                None => Err(kept
                    .iter()
                    .find(|entry| entry.record.running())
                    .map(|entry| entry.record.clone())),
            }
        })?;
        match done {
            Ok(record) => return finished(record),
            Err(running) if deadline.is_expired() => {
                return Err(no_download(deadline, running.as_ref()));
            }
            Err(_) => std::thread::sleep(WAIT_POLL.min(deadline.remaining())),
        }
    }
}

fn finished(record: DownloadRecord) -> Result<DownloadRecord, AppError> {
    match record.state {
        DownloadState::Canceled | DownloadState::TooLarge { .. } => Err(AppError::new(
            ErrorCode::ActionFailed,
            download_line(&record),
        )
        .with_details(json!({ "kind": "download_canceled", "download": download_json(&record) }))
        .with_disposition(DeliverySemantics::delivered_unverified())),
        _ => Ok(record),
    }
}

fn no_download(deadline: Deadline, running: Option<&DownloadRecord>) -> AppError {
    let waited = deadline.elapsed().as_millis();
    let message = match running {
        Some(record) => format!("{} after {waited} ms", download_line(record)),
        None => format!("no download started or finished within {waited} ms"),
    };
    AppError::timeout(message)
        .with_suggestion(
            "Click the control that starts the download first, or pass a longer --timeout",
        )
        .with_details(json!({ "kind": "download_timeout", "timeout_ms": deadline.timeout_ms() }))
}
