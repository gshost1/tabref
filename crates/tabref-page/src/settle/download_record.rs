use serde::{Deserialize, Serialize};

use super::download_state::DownloadState;

/// One download the browser announced: its id, the address it came from
/// and the file name the server suggested, where it stands, the bytes
/// received, where the file is once known, and when it began and finished
/// (milliseconds since the Unix epoch).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub guid: String,
    pub source: (String, String),
    pub state: DownloadState,
    pub bytes: u64,
    pub path: Option<String>,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
}

impl DownloadRecord {
    /// The address the download came from.
    pub fn url(&self) -> &str {
        &self.source.0
    }

    /// The file name the server suggested.
    pub fn suggested_name(&self) -> &str {
        &self.source.1
    }

    /// How long the download took, once it finished.
    pub fn elapsed_ms(&self) -> Option<u64> {
        self.finished_ms
            .map(|finished| finished.saturating_sub(self.started_ms))
    }
}
