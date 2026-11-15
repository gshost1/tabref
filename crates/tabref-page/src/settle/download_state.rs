use serde::{Deserialize, Serialize};

/// Where a download stands: still receiving, saved whole, stopped by the
/// browser or the page, or stopped by tabref for passing the size cap (in
/// megabytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    InProgress,
    Completed,
    Canceled,
    TooLarge { cap_mb: u64 },
}

impl DownloadState {
    /// The state a `Browser.downloadProgress` event names.
    pub(crate) fn from_cdp(text: &str) -> Option<Self> {
        match text {
            "inProgress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "canceled" => Some(Self::Canceled),
            _ => None,
        }
    }
}
