use std::path::PathBuf;

/// How to start a browser: headless or with a window, which binary, the
/// directory its throwaway profile goes in, a persistent profile to use
/// instead (`open --launch --profile DIR`), and extra flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    pub headed: bool,
    pub binary: Option<PathBuf>,
    pub profile_root: Option<PathBuf>,
    pub profile_dir: Option<PathBuf>,
    pub extra_args: Vec<String>,
}
