use serde::{Deserialize, Serialize};

use super::download_origin::DownloadOrigin;
use super::download_state::DownloadState;
use super::download_timing::DownloadTiming;

/// One download the browser announced: its id, where it came from, where it
/// stands, the bytes received and the size the server announced (0 when it
/// did not), where the file is once known, and when it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRecord {
    pub guid: String,
    pub origin: DownloadOrigin,
    pub state: DownloadState,
    pub bytes: u64,
    pub expected: u64,
    pub path: Option<String>,
    pub timing: DownloadTiming,
}

impl DownloadRecord {
    /// Whether the download is still receiving.
    pub fn running(&self) -> bool {
        self.state == DownloadState::InProgress
    }

    /// Marks the download finished in `state` at `now`.
    pub fn finish(&mut self, state: DownloadState, now: u64) {
        self.state = state;
        self.timing.finished_ms = Some(now);
    }
}
