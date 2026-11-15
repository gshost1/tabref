//! Downloads around actions and `wait --download`: the browser's download
//! events turned on before an action, the downloads it started followed
//! for a moment and named, and the ones still running kept for a later
//! wait.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics};
use tabref_page::{
    DownloadRecord, DownloadState, enable_downloads, next_download_event, note_downloads, now_ms,
};

use super::download_folder::{download_folder, load_ledger, save_ledger};
use super::download_place::{landed_size, place};
use crate::session::Session;
use crate::view::node_line::quote_text;

const LANDING: Duration = Duration::from_millis(3_000);
const DISK_POLL: Duration = Duration::from_millis(250);
const NAVIGATION_GRACE: Duration = Duration::from_millis(1_000);

/// Turns the browser's download events on, pointed at this browser's
/// download folder when it has one.
pub(crate) fn enable(session: &mut Session, deadline: Deadline) -> Result<(), AppError> {
    let folder = download_folder(session);
    enable_downloads(session.connection(), folder.as_deref(), deadline)
}

/// The downloads the action just started, each followed for up to three
/// seconds (at most half of what is left of `deadline`) and named once it
/// lands. With `grace`, a download that has not announced itself yet is
/// waited for that long, as after a navigation the browser turned into a
/// download. Every one is kept for `wait --download`. A failure to read or
/// keep them never fails the action.
pub(crate) fn after_action(
    session: &mut Session,
    deadline: Deadline,
    grace: Duration,
) -> Vec<DownloadRecord> {
    let mut records = Vec::new();
    let noted = note_downloads(session.connection(), &mut records, deadline).unwrap_or(false);
    let heard = noted
        || (!grace.is_zero()
            && next_download_event(session.connection(), &mut records, deadline.capped(grace))
                .unwrap_or(false));
    if !heard {
        return records;
    }
    let landing = deadline.capped(LANDING.min(deadline.remaining() / 2));
    while records.iter().any(running) {
        match next_download_event(session.connection(), &mut records, landing) {
            Ok(true) => {}
            _ => break,
        }
    }
    let folder = download_folder(session);
    for record in &mut records {
        name_landed(record, folder.as_deref());
    }
    let mut ledger = load_ledger(session);
    ledger.extend(records.iter().cloned());
    let _ = save_ledger(session, &ledger);
    records
}

/// How long to wait for a download to announce itself after a
/// navigation: a second when the browser gave the navigation up, as it does
/// when the address turns out to be a download, otherwise not at all.
pub(crate) fn grace_after(navigated: &Result<(), AppError>) -> Duration {
    match navigated {
        Err(error) if error.kind() == Some("cdp_navigation_failed") => NAVIGATION_GRACE,
        _ => Duration::ZERO,
    }
}

/// Waits within `deadline` for the oldest download `wait --download` has
/// not returned yet to finish, and returns it: a download an earlier
/// command started (found in the folder when its events were missed) or
/// one that starts during the wait.
pub(crate) fn wait_download(
    session: &mut Session,
    deadline: Deadline,
) -> Result<DownloadRecord, AppError> {
    enable(session, deadline)?;
    let folder = download_folder(session);
    let mut ledger = load_ledger(session);
    note_downloads(session.connection(), &mut ledger, deadline)?;
    loop {
        for record in &mut ledger {
            if running(record)
                && let Some(size) = folder
                    .as_deref()
                    .and_then(|dir| landed_size(dir, &record.guid))
            {
                record.state = DownloadState::Completed;
                record.bytes = size;
                record.finished_ms = Some(now_ms());
            }
            name_landed(record, folder.as_deref());
        }
        if let Some(index) = ledger.iter().position(|record| !running(record)) {
            let done = ledger.remove(index);
            save_ledger(session, &ledger)?;
            return finished(done);
        }
        let slice = deadline.capped(DISK_POLL);
        let heard = next_download_event(session.connection(), &mut ledger, slice)?;
        if !heard && deadline.is_expired() {
            save_ledger(session, &ledger)?;
            return Err(no_download(deadline, &ledger));
        }
    }
}

/// One line for a download: `downloaded "report.csv" (24 bytes) -> PATH`,
/// or that it is still running or was canceled.
pub(crate) fn download_line(record: &DownloadRecord) -> String {
    let name = quote_text(&file_name(record));
    match (record.state, &record.path) {
        (DownloadState::Completed, Some(path)) => {
            format!("downloaded {name} ({} bytes) -> {path}", record.bytes)
        }
        (DownloadState::Completed, None) => format!(
            "downloaded {name} ({} bytes), saved where this browser keeps its downloads",
            record.bytes
        ),
        (DownloadState::InProgress, _) => {
            format!("download {name} still running; tabref wait --download")
        }
        (DownloadState::Canceled, _) => format!("download {name} was canceled"),
    }
}

/// A download as `--json` gives it.
pub(crate) fn download_json(record: &DownloadRecord) -> Value {
    json!({
        "name": file_name(record),
        "path": record.path,
        "url": record.url(),
        "bytes": record.bytes,
        "state": record.state,
        "elapsed_ms": record.elapsed_ms(),
    })
}

fn finished(record: DownloadRecord) -> Result<DownloadRecord, AppError> {
    if record.state == DownloadState::Canceled {
        return Err(
            AppError::new(tabref_core::ErrorCode::ActionFailed, download_line(&record))
                .with_details(
                    json!({ "kind": "download_canceled", "download": download_json(&record) }),
                )
                .with_disposition(DeliverySemantics::delivered_unverified()),
        );
    }
    Ok(record)
}

fn no_download(deadline: Deadline, ledger: &[DownloadRecord]) -> AppError {
    let message = match ledger.iter().find(|record| running(record)) {
        Some(record) => format!(
            "{} after {} ms",
            download_line(record),
            deadline.elapsed().as_millis()
        ),
        None => format!(
            "no download started or finished within {} ms",
            deadline.elapsed().as_millis()
        ),
    };
    AppError::timeout(message)
        .with_suggestion(
            "Click the control that starts the download first, or pass a longer --timeout",
        )
        .with_details(json!({ "kind": "download_timeout", "timeout_ms": deadline.timeout_ms() }))
}

/// Gives a landed guid-named download its suggested name in `folder`; one
/// the browser saved under its own name keeps it.
fn name_landed(record: &mut DownloadRecord, folder: Option<&Path>) {
    let Some(folder) = folder else {
        return;
    };
    if record.state != DownloadState::Completed {
        return;
    }
    let guid_path = folder.join(&record.guid).to_string_lossy().into_owned();
    if record.path.as_deref().is_some_and(|path| path != guid_path) {
        return;
    }
    let placed = place(folder, &record.guid, record.suggested_name());
    record.path = Some(placed.map_or(guid_path, |path| path.to_string_lossy().into_owned()));
}

fn file_name(record: &DownloadRecord) -> String {
    record
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| record.suggested_name().to_owned())
}

fn running(record: &DownloadRecord) -> bool {
    record.state == DownloadState::InProgress
}
