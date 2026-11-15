use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline};

use super::download_origin::DownloadOrigin;
use super::download_record::DownloadRecord;
use super::download_state::DownloadState;
use super::download_timing::DownloadTiming;
use crate::{CdpConnection, CdpEvent};

const WILL_BEGIN: &str = "Browser.downloadWillBegin";
const PROGRESS: &str = "Browser.downloadProgress";

/// Turns the browser's download events on for this connection. With a
/// folder the browser saves each download there under its guid
/// (`allowAndName`), for the caller to give it its name; without one it
/// keeps its own folder and naming, so the downloads of a browser tabref
/// did not start are never redirected.
pub fn enable_downloads(
    connection: &mut CdpConnection,
    folder: Option<&Path>,
    deadline: Deadline,
) -> Result<(), AppError> {
    let params = match folder {
        Some(folder) => json!({
            "behavior": "allowAndName",
            "downloadPath": folder.to_string_lossy(),
            "eventsEnabled": true,
        }),
        None => json!({ "behavior": "default", "eventsEnabled": true }),
    };
    connection
        .call("Browser.setDownloadBehavior", params, None, deadline)
        .map(|_| ())
}

/// Gives the browser back its own download behaviour, with no events for
/// this connection, so the user's later downloads stay theirs.
pub fn restore_downloads(
    connection: &mut CdpConnection,
    deadline: Deadline,
) -> Result<(), AppError> {
    connection
        .call(
            "Browser.setDownloadBehavior",
            json!({ "behavior": "default" }),
            None,
            deadline,
        )
        .map(|_| ())
}

/// Folds the download events the connection already holds, after reading
/// every frame the browser sent, into `records`. Returns whether any came.
pub fn note_downloads(
    connection: &mut CdpConnection,
    records: &mut Vec<DownloadRecord>,
    deadline: Deadline,
) -> Result<bool, AppError> {
    connection.poll_pending(deadline)?;
    let events = connection.drain_events(is_download_event);
    let now = now_ms();
    for event in &events {
        apply_event(records, event, now);
    }
    Ok(!events.is_empty())
}

/// Waits for the next download event and folds it into `records`.
/// Returns `false` when `deadline` passed first.
pub fn next_download_event(
    connection: &mut CdpConnection,
    records: &mut Vec<DownloadRecord>,
    deadline: Deadline,
) -> Result<bool, AppError> {
    match connection.wait_for_event(is_download_event, deadline) {
        Ok(event) => {
            apply_event(records, &event, now_ms());
            Ok(true)
        }
        Err(error) if is_timeout(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Stops the download `guid`. A download that already ended is left as it
/// is.
pub fn cancel_download(
    connection: &mut CdpConnection,
    guid: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    connection
        .call(
            "Browser.cancelDownload",
            json!({ "guid": guid }),
            None,
            deadline,
        )
        .map(|_| ())
}

/// Whether `event` is one of the browser's own download events.
pub(crate) fn is_download_event(event: &CdpEvent) -> bool {
    event.session_id.is_none() && (event.method == WILL_BEGIN || event.method == PROGRESS)
}

/// Folds one download event into `records`, at `now` (milliseconds since
/// the epoch). A start adds a record once per guid; progress updates its
/// bytes, its path once the browser names one, and its state. A finished
/// download never goes back to in progress.
pub(crate) fn apply_event(records: &mut Vec<DownloadRecord>, event: &CdpEvent, now: u64) {
    let params = &event.params;
    let Some(guid) = params.get("guid").and_then(Value::as_str) else {
        return;
    };
    if event.method == WILL_BEGIN {
        if records.iter().all(|record| record.guid != guid) {
            records.push(DownloadRecord {
                guid: guid.to_owned(),
                origin: DownloadOrigin {
                    url: text(params, "url"),
                    suggested_name: text(params, "suggestedFilename"),
                    frame_id: text(params, "frameId"),
                },
                state: DownloadState::InProgress,
                bytes: 0,
                expected: 0,
                path: None,
                timing: DownloadTiming {
                    started_ms: now,
                    finished_ms: None,
                },
            });
        }
        return;
    }
    let Some(record) = records.iter_mut().find(|record| record.guid == guid) else {
        return;
    };
    if !record.running() {
        return;
    }
    if let Some(bytes) = params.get("receivedBytes").and_then(Value::as_u64) {
        record.bytes = bytes;
    }
    if let Some(total) = params.get("totalBytes").and_then(Value::as_u64) {
        record.expected = total;
    }
    if let Some(path) = params.get("filePath").and_then(Value::as_str) {
        record.path = Some(path.to_owned());
    }
    if let Some(state) = params
        .get("state")
        .and_then(Value::as_str)
        .and_then(DownloadState::from_cdp)
        && state != DownloadState::InProgress
    {
        record.finish(state, now);
    }
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

fn text(params: &Value, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
#[path = "download_events_tests.rs"]
mod tests;
