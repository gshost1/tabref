use std::path::PathBuf;

/// How to start a browser: headless or with a window, which binary, the
/// directory its throwaway profile goes in, and extra flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    pub headed: bool,
    pub binary: Option<PathBuf>,
    pub profile_root: Option<PathBuf>,
    pub extra_args: Vec<String>,
}
