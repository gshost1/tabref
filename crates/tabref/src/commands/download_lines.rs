//! How a download reads in a result: one line, and its `--json` shape.

use std::path::Path;

use serde_json::{Value, json};
use tabref_page::{DownloadRecord, DownloadState};

use crate::view::node_line::quote_text;

/// One line for a download: `downloaded "report.csv" (24 bytes) -> PATH`,
/// or that it is still running, was canceled, or passed the size cap.
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
        (DownloadState::TooLarge { cap_mb }, _) => format!(
            "download {name} canceled at {} bytes: over the {cap_mb} MB download cap; raise it with open --download-max-mb",
            record.bytes
        ),
    }
}

/// A download as `--json` gives it.
pub(crate) fn download_json(record: &DownloadRecord) -> Value {
    json!({
        "name": file_name(record),
        "path": record.path,
        "url": record.origin.url,
        "bytes": record.bytes,
        "state": record.state,
        "elapsed_ms": record.timing.elapsed_ms(),
    })
}

/// The name the file has in its folder, or the one the server suggested
/// while it has none.
pub(crate) fn file_name(record: &DownloadRecord) -> String {
    record
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| record.origin.suggested_name.clone())
}
