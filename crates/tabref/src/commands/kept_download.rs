use serde::{Deserialize, Serialize};
use tabref_page::DownloadRecord;

/// A download the kept list remembers: the download, whether a result has
/// reported it finished, whether a tabref action or wait saw it begin
/// (only those are capped or canceled), and the batch whose step reported
/// it, which that batch's `expect download` may still return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct KeptDownload {
    pub(crate) record: DownloadRecord,
    pub(crate) reported: bool,
    pub(crate) watched: bool,
    pub(crate) batch: Option<u64>,
}

impl KeptDownload {
    /// Whether a wait in `batch` may return it: it finished, and no result
    /// reported it, or a step of that same batch did.
    pub(crate) fn claimable(&self, batch: Option<u64>) -> bool {
        !self.record.running() && (!self.reported || (batch.is_some() && self.batch == batch))
    }
}
