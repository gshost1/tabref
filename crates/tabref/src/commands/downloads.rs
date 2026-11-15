#![cfg_attr(windows, allow(dead_code))]
//! Downloads around actions: the browser's download events turned on for a
//! command (and, for a browser tabref did not launch, off again when the
//! last command on it ends), and the downloads an action started followed
//! for a moment, capped and named, each kept with a mark of whether the
//! action reported it.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use tabref_cdp::{CdpConnection, PageSession, list_frames};
use tabref_core::{AppError, Deadline};
use tabref_page::{
    DownloadRecord, DownloadState, cancel_download, deny_downloads, enable_downloads,
    next_download_event, note_downloads, now_ms, restore_downloads,
};

use super::download_folder::{UNSAFE_FOLDER, download_cap, download_folder, launched_here};
use super::download_ledger::{DownloadLedger, merge};
use super::download_place::{landed_size, place};
use super::kept_download::KeptDownload;
use crate::session::Session;

const LANDING: Duration = Duration::from_millis(3_000);
const NAVIGATION_GRACE: Duration = Duration::from_millis(1_000);
const RESTORING: u64 = 300;
const MB: u64 = 1024 * 1024;

static ACTIVE: Mutex<BTreeMap<u16, usize>> = Mutex::new(BTreeMap::new());

thread_local! {
    static BATCH: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Turns the browser's download events on, pointed at this browser's
/// download folder when it has one. A chosen folder that is no longer safe
/// makes the browser refuse downloads and is the error.
pub(crate) fn enable(session: &mut Session, deadline: Deadline) -> Result<(), AppError> {
    match download_folder(session) {
        Ok(folder) => enable_downloads(session.connection(), folder.as_deref(), deadline),
        Err(error) => {
            let _ = deny_downloads(session.connection(), deadline);
            Err(error)
        }
    }
}

/// [`enable`] before an action or an open: only an unsafe folder fails the
/// command; a browser that refuses download events (an embedded runtime)
/// does not.
pub(crate) fn enable_for_action(session: &mut Session, deadline: Deadline) -> Result<(), AppError> {
    match enable(session, deadline) {
        Err(error) if error.kind() == Some(UNSAFE_FOLDER) => Err(error),
        _ => Ok(()),
    }
}

/// Notes that a daemon command started on this session's browser.
pub(crate) fn begin_command(session: &Session) {
    if let Ok(mut active) = ACTIVE.lock() {
        *active.entry(session.endpoint().port()).or_default() += 1;
    }
}

/// Notes that a daemon command ended. When it was the last one running on
/// a browser tabref did not launch, that browser gets its default download
/// behaviour back with no events, so the user's own downloads are never
/// redirected, capped or reported. A download already running keeps its
/// file.
pub(crate) fn end_command(session: &mut Session) {
    let port = session.endpoint().port();
    let last = ACTIVE.lock().is_ok_and(|mut active| {
        let count = active.entry(port).or_default();
        *count = count.saturating_sub(1);
        *count == 0
    });
    if !last || launched_here(session) {
        return;
    }
    if let Ok(deadline) = Deadline::after(RESTORING) {
        let _ = restore_downloads(session.connection(), deadline);
    }
}

/// Runs `steps` as one batch: a download a step reports may still be
/// returned by that batch's `expect download`.
pub(crate) fn in_batch<T>(batch: u64, steps: impl FnOnce() -> T) -> T {
    let outer = BATCH.with(|cell| cell.replace(Some(batch)));
    let result = steps();
    BATCH.with(|cell| cell.set(outer));
    result
}

/// The batch this thread runs, if any.
pub(crate) fn current_batch() -> Option<u64> {
    BATCH.with(Cell::get)
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
/// tab's page started is not this action's and is kept unreported. `opened`
/// is a tab the action opened, whose downloads count as the action's. A
/// failure here never fails the action.
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
    let folder = download_folder(session).ok().flatten();
    let elsewhere = other_tabs(session, tab, opened, deadline);
    let landing = deadline.capped(LANDING.min(deadline.remaining() / 2));
    loop {
        let (mut here, rest): (Vec<_>, Vec<_>) = records
            .into_iter()
            .partition(|record| !elsewhere.contains(&record.origin.frame_id));
        enforce_cap(
            session.connection(),
            &mut here,
            cap,
            folder.as_deref(),
            deadline,
        );
        let running = here.iter().any(DownloadRecord::running);
        records = here.into_iter().chain(rest).collect();
        if !running {
            break;
        }
        if !next_download_event(session.connection(), &mut records, landing).unwrap_or(false) {
            break;
        }
    }
    for record in &mut records {
        name_landed(record, folder.as_deref());
    }
    let batch = current_batch();
    let kept: Vec<KeptDownload> = records
        .iter()
        .map(|record| {
            let here = !elsewhere.contains(&record.origin.frame_id);
            KeptDownload {
                record: record.clone(),
                reported: here && !record.running(),
                watched: here,
                batch: batch.filter(|_| here),
            }
        })
        .collect();
    let ledger = DownloadLedger::of(session);
    let _ = ledger.with(|ledger| {
        for entry in kept {
            merge(ledger, entry);
        }
    });
    records
        .into_iter()
        .filter(|record| !elsewhere.contains(&record.origin.frame_id))
        .collect()
}

/// Whether the download received, or announced, more than `cap` bytes.
pub(crate) fn past_cap(record: &DownloadRecord, cap: u64) -> bool {
    record.bytes.max(record.expected) > cap
}

/// Stops every download past `cap` bytes, received or announced: a
/// running one is canceled, and one that finished before tabref saw it
/// pass the cap has its file in `folder` removed.
pub(crate) fn enforce_cap(
    connection: &mut CdpConnection,
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
                if let Some(dir) = folder {
                    let file = record
                        .path
                        .clone()
                        .unwrap_or_else(|| dir.join(&record.guid).to_string_lossy().into_owned());
                    if Path::new(&file).starts_with(dir) {
                        let _ = std::fs::remove_file(file);
                    }
                }
                record.path = None;
            }
            _ => continue,
        }
        record.finish(too_large, now_ms());
    }
}

/// Marks a running download finished when its file landed in `folder`,
/// for one whose events this process never saw.
pub(crate) fn land_from_disk(record: &mut DownloadRecord, folder: Option<&Path>) {
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
pub(crate) fn name_landed(record: &mut DownloadRecord, folder: Option<&Path>) {
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

#[cfg(test)]
#[path = "downloads_tests.rs"]
mod tests;
