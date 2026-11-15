use std::path::PathBuf;

/// What the listener check knows of one process of this user: its id, the
/// binary it runs and its command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessView {
    pub pid: u32,
    pub exe: PathBuf,
    pub cmdline: Vec<String>,
}

impl ProcessView {
    /// The binary's file name, without the ` (deleted)` Linux appends once a
    /// running binary was replaced on disk, as a browser update does.
    pub fn exe_name(&self) -> String {
        let name = self
            .exe
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        name.strip_suffix(" (deleted)").unwrap_or(&name).to_owned()
    }
}
