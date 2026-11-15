use std::path::PathBuf;

/// What `open` was asked to do besides loading its address: load it in a
/// new tab, choose or forget this browser's download folder, and set the
/// download size cap in megabytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OpenRequest {
    pub(crate) new_tab: bool,
    pub(crate) download_dir: Option<PathBuf>,
    pub(crate) clear_download_dir: bool,
    pub(crate) download_max_mb: Option<u64>,
}
