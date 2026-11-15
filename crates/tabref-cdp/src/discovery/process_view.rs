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

    /// The command line as arguments. A process that rewrote its title
    /// (headless Chrome does) shows its whole command line as one string;
    /// that string is split before each ` --`, so its flags read as they
    /// were passed. Arguments that arrived separately are kept whole, spaces
    /// and all.
    pub fn args(&self) -> Vec<String> {
        if self.cmdline.len() != 1 {
            return self.cmdline.clone();
        }
        let mut parts = self.cmdline[0].split(" --");
        let mut args: Vec<String> = parts.next().map(str::to_owned).into_iter().collect();
        args.extend(parts.map(|part| format!("--{part}")));
        args
    }
}

#[cfg(test)]
#[path = "process_view_tests.rs"]
mod tests;
