use serde::{Deserialize, Serialize};

/// One request line from the client to the daemon. `op` is `run`, `status`
/// or `stop`. A `run` carries the command line as typed (without the
/// program name), the client's working directory for relative paths, and
/// the endpoint the client resolved from `--cdp` or `TABREF_CDP`, the
/// command's path arguments as the client resolved them (see
/// `request_paths`), and the command's standard input for a command that
/// reads one. `build` names the client binary, so a daemon of another build
/// steps aside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WireRequest {
    pub(crate) op: String,
    pub(crate) build: String,
    #[serde(default)]
    pub(crate) argv: Vec<String>,
    #[serde(default)]
    pub(crate) cwd: String,
    #[serde(default)]
    pub(crate) cdp: Option<String>,
    #[serde(default)]
    pub(crate) paths: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) stdin: Option<String>,
}

impl WireRequest {
    pub(crate) fn control(op: &str, build: String) -> Self {
        Self {
            op: op.to_owned(),
            build,
            argv: Vec::new(),
            cwd: String::new(),
            cdp: None,
            paths: None,
            stdin: None,
        }
    }
}
