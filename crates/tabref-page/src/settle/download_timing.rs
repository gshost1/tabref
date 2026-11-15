use serde::{Deserialize, Serialize};

/// When a download began and, once it did, finished, in milliseconds since
/// the Unix epoch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadTiming {
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
}

impl DownloadTiming {
    /// How long the download took, once it finished.
    pub fn elapsed_ms(self) -> Option<u64> {
        self.finished_ms
            .map(|finished| finished.saturating_sub(self.started_ms))
    }
}
