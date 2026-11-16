use tabref_cdp::{DialogChoice, LaunchOptions};

/// What a command asks of its session: the endpoint `--cdp` names, the tab
/// `--tab` names, the deadline, how to start a browser when none is
/// reachable (`open --launch`), how to answer the first dialog (`--dialog`),
/// and, inside the daemon, the standard input the client sent for the
/// command.
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionOptions {
    pub(crate) cdp: Option<String>,
    pub(crate) tab: Option<String>,
    pub(crate) timeout_ms: u64,
    pub(crate) launch: Option<LaunchOptions>,
    pub(crate) stdin: Option<String>,
    pub(crate) dialog: Option<DialogChoice>,
}
