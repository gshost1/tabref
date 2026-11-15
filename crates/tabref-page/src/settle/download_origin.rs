use serde::{Deserialize, Serialize};

/// Where a download came from: its address, the file name the server
/// suggested, and the frame that started it (empty when the browser did not
/// say).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadOrigin {
    pub url: String,
    pub suggested_name: String,
    pub frame_id: String,
}
