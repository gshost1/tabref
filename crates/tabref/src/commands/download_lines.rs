//! How a download reads in a result: one line, and its `--json` shape.

use std::path::Path;

use serde_json::{Value, json};
use tabref_page::{DownloadRecord, DownloadState};

use crate::output::outcome::Outcome;
use crate::view::node_line::quote_text;

impl Extend<DownloadRecord> for Outcome {
    fn extend<T: IntoIterator<Item = DownloadRecord>>(&mut self, records: T) {
        for record in records {
            let download = download_json(&record);
            if self
                .data
                .get("downloads")
                .and_then(Value::as_array)
                .is_some_and(|records| records.contains(&download))
            {
                continue;
            }
            self.push_line(&download_line(&record));
            if let Some(data) = self.data.as_object_mut() {
                let downloads = data.entry("downloads").or_insert_with(|| json!([]));
                if let Some(downloads) = downloads.as_array_mut() {
                    downloads.push(download.clone());
                }
            }
        }
    }
}

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
        (DownloadState::TooLarge { cap_mb }, _) => {
            let cleanup = super::downloads::cleanup::lookup(record).unwrap_or_default();
            let action = if !cleanup.pending_paths.is_empty() {
                format!("delete pending: {}", cleanup.pending_paths.join(", "))
            } else if cleanup.completed_before_cap {
                if cleanup.removal_verified {
                    "removed".into()
                } else {
                    "removal unverified (download folder unknown)".into()
                }
            } else {
                "canceled".into()
            };
            format!(
                "download {name} {action} at {} bytes: over the {cap_mb} MB download cap; raise it with open --download-max-mb",
                record.bytes
            )
        }
    }
}

/// A download as `--json` gives it.
pub(crate) fn download_json(record: &DownloadRecord) -> Value {
    let mut data = json!({
        "name": file_name(record),
        "path": record.path,
        "url": record.origin.url,
        "bytes": record.bytes,
        "state": record.state,
        "elapsed_ms": record.timing.elapsed_ms(),
    });
    if let Some(cleanup) = super::downloads::cleanup::lookup(record) {
        data["completed_before_cap"] = json!(cleanup.completed_before_cap);
        data["removal_pending"] = json!(!cleanup.pending_paths.is_empty());
    }
    data
}

/// The name the file has in its folder, or the one the server suggested
/// while it has none.
pub(crate) fn file_name(record: &DownloadRecord) -> String {
    if matches!(record.state, DownloadState::TooLarge { .. }) {
        return record.origin.suggested_name.clone();
    }
    record
        .path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| record.origin.suggested_name.clone())
}
