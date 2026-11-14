use tabref_cdp::LaunchOptions;

/// What a command asks of its session: the endpoint `--cdp` names, the tab
/// `--tab` names, the deadline, and how to start a browser when none is
/// reachable (`open --launch`).
#[derive(Debug, Clone, Default)]
pub(crate) struct SessionOptions {
    pub(crate) cdp: Option<String>,
    pub(crate) tab: Option<String>,
    pub(crate) timeout_ms: u64,
    pub(crate) launch: Option<LaunchOptions>,
}
