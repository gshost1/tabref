//! Downloads around actions, between commands and in `wait --download`:
//! the browser's download events turned on for a command, the downloads an
//! action started followed for a moment, capped and named, and those it
//! did not see finish kept for a later wait. A browser tabref did not launch
//! is pointed at a chosen folder only while a command runs.

use std::path::Path;
use std::time::Duration;

use tabref_cdp::{PageSession, list_frames};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};
use tabref_page::{
    DownloadRecord, DownloadState, cancel_download, enable_downloads, next_download_event,
    note_downloads, now_ms, restore_downloads,
};

use super::download_folder::{download_cap, download_folder, folder_chosen, launched_here};
use super::download_ledger::{DownloadLedger, merge};
use super::download_lines::{download_json, download_line};
use super::download_place::{landed_size, place};
use crate::session::Session;
use serde_json::json;

const LANDING: Duration = Duration::from_millis(3_000);
const WAIT_POLL: Duration = Duration::from_millis(50);
const NAVIGATION_GRACE: Duration = Duration::from_millis(1_000);
const TENDING: u64 = 300;
const MB: u64 = 1024 * 1024;

/// Turns the browser's download events on, pointed at this browser's
/// download folder when it has one.
pub(crate) fn enable(session: &mut Session, deadline: Deadline) -> Result<(), AppError> {
    let folder = download_folder(session);
    enable_downloads(session.connection(), folder.as_deref(), deadline)
}

/// At the end of a daemon command: a browser tabref did not launch gets
/// its own download behaviour back, so the user's later downloads never
/// land in tabref's folder. A download already running keeps its file.
pub(crate) fn end_command(session: &mut Session) {
    if launched_here(session) || !folder_chosen(session) {
        return;
    }
    if let Ok(deadline) = Deadline::after(TENDING) {
        let _ = restore_downloads(session.connection(), deadline);
    }
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

/// The downloads the action in `tab` started, each followed for up to
/// three seconds (at most half of what is left of `deadline`), stopped at
/// the size cap and named once it lands. With `grace`, a download that has
/// not announced itself yet is waited for that long. A download another
/// tab's page started is not this action's: it is kept for a later wait,
/// as is one still running. `opened` is a tab the action opened, whose
/// downloads count as the action's. A failure here never fails the action.
pub(crate) fn after_action(
    session: &mut Session,
    tab: &PageSession,
    opened: Option<&str>,
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
    let cap = download_cap(session);
    let folder = download_folder(session);
    let landing = deadline.capped(LANDING.min(deadline.remaining() / 2));
    loop {
        enforce_cap(
            session.connection(),
            &mut records,
            cap,
            folder.as_deref(),
            deadline,
        );
        if !records.iter().any(DownloadRecord::running) {
            break;
        }
        if !next_download_event(session.connection(), &mut records, landing).unwrap_or(false) {
            break;
        }
    }
    for record in &mut records {
        name_landed(record, folder.as_deref());
    }
    let elsewhere = other_tabs(session, tab, opened, deadline);
    let (here, kept): (Vec<_>, Vec<_>) = records
        .into_iter()
        .partition(|record| !elsewhere.contains(&record.origin.frame_id));
    let ledger = DownloadLedger::of(session);
    let _ = ledger.with(|ledger| {
        for record in here.iter().filter(|record| record.running()) {
            merge(ledger, record.clone());
        }
        for record in kept {
            merge(ledger, record);
        }
    });
    here
}

/// Between daemon commands: folds the download events this connection
/// got into the kept list, stops a download past the cap, and names every
/// one that landed, so no download stays under its guid.
pub(crate) fn tend(session: &mut Session) {
    let Ok(deadline) = Deadline::after(TENDING) else {
        return;
    };
    let folder = download_folder(session);
    let cap = download_cap(session);
    let ledger = DownloadLedger::of(session);
    let _ = ledger.with(|records| {
        let _ = note_downloads(session.connection(), records, deadline);
        enforce_cap(
            session.connection(),
            records,
            cap,
            folder.as_deref(),
            deadline,
        );
        for record in records.iter_mut() {
            land_from_disk(record, folder.as_deref());
            name_landed(record, folder.as_deref());
        }
    });
}

/// When the daemon retires: cancels the downloads still running, as no
/// process will be left to name them.
pub(crate) fn retire(session: &mut Session) {
    let Ok(deadline) = Deadline::after(TENDING) else {
        return;
    };
    let ledger = DownloadLedger::of(session);
    let _ = ledger.with(|records| {
        for record in records.iter_mut().filter(|record| record.running()) {
            let _ = cancel_download(session.connection(), &record.guid, deadline);
            record.finish(DownloadState::Canceled, now_ms());
        }
    });
}

/// Waits within `deadline` for the oldest download no command has
/// reported finished, and returns it: one an earlier command left running
/// (found in the folder when its events were missed), one that finished
/// between commands, or one that starts during the wait.
pub(crate) fn wait_download(
    session: &mut Session,
    deadline: Deadline,
) -> Result<DownloadRecord, AppError> {
    enable(session, deadline)?;
    let folder = download_folder(session);
    let cap = download_cap(session);
    let ledger = DownloadLedger::of(session);
    loop {
        let done = ledger.with(|records| {
            let _ = note_downloads(session.connection(), records, deadline);
            enforce_cap(
                session.connection(),
                records,
                cap,
                folder.as_deref(),
                deadline,
            );
            for record in records.iter_mut() {
                land_from_disk(record, folder.as_deref());
                name_landed(record, folder.as_deref());
            }
            let finished = records.iter().position(|record| !record.running());
            match finished {
                Some(index) => Ok(records.remove(index)),
                None => Err(records.iter().find(|record| record.running()).cloned()),
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

/// Stops every download past `cap` bytes, received or announced: a
/// running one is canceled, and one that finished before tabref saw it
/// pass the cap has its file in `folder` removed.
fn enforce_cap(
    connection: &mut tabref_cdp::CdpConnection,
    records: &mut [DownloadRecord],
    cap: u64,
    folder: Option<&Path>,
    deadline: Deadline,
) {
    let too_large = DownloadState::TooLarge { cap_mb: cap / MB };
    for record in records.iter_mut().filter(|record| past_cap(record, cap)) {
        match record.state {
            DownloadState::InProgress => {
                let _ = cancel_download(connection, &record.guid, deadline);
            }
            DownloadState::Completed => {
                let file = record.path.clone().or_else(|| {
                    folder.map(|dir| dir.join(&record.guid).to_string_lossy().into_owned())
                });
                if let Some(file) =
                    file.filter(|file| folder.is_some_and(|dir| Path::new(file).starts_with(dir)))
                {
                    let _ = std::fs::remove_file(file);
                }
                record.path = None;
            }
            _ => continue,
        }
        record.finish(too_large, now_ms());
    }
}

/// Whether the download received, or announced, more than `cap` bytes.
pub(crate) fn past_cap(record: &DownloadRecord, cap: u64) -> bool {
    record.bytes.max(record.expected) > cap
}

/// The ids of the other open tabs, whose main frames carry them: a
/// download one of them started is not this action's. `tab` and `opened`
/// are left out.
fn other_tabs(
    session: &mut Session,
    tab: &PageSession,
    opened: Option<&str>,
    deadline: Deadline,
) -> Vec<String> {
    let own: Vec<String> = list_frames(session.connection(), tab, deadline)
        .map(|frames| frames.into_iter().map(|frame| frame.frame_id).collect())
        .unwrap_or_default();
    session
        .tabs()
        .map(|tabs| {
            tabs.into_iter()
                .map(|listed| listed.target_id)
                .filter(|id| {
                    *id != tab.target_id && Some(id.as_str()) != opened && !own.contains(id)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Marks a running download finished when its file landed in `folder`,
/// for one whose events this process never saw.
fn land_from_disk(record: &mut DownloadRecord, folder: Option<&Path>) {
    if !record.running() {
        return;
    }
    if let Some(size) = folder.and_then(|dir| landed_size(dir, &record.guid)) {
        record.bytes = size;
        record.finish(DownloadState::Completed, now_ms());
    }
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
    let placed = place(folder, &record.guid, &record.origin.suggested_name);
    record.path = Some(placed.map_or(guid_path, |path| path.to_string_lossy().into_owned()));
}

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
