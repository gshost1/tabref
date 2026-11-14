use serde::{Deserialize, Serialize};

/// One request line from the client to the daemon. `op` is `run`, `status`
/// or `stop`. A `run` carries the command line as typed (without the
/// program name), the client's working directory for relative paths, and
/// the endpoint the client resolved from `--cdp` or `TABREF_CDP`. `build`
/// names the client binary, so a daemon of another build steps aside.
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
}

impl WireRequest {
    pub(crate) fn control(op: &str, build: String) -> Self {
        Self {
            op: op.to_owned(),
            build,
            argv: Vec::new(),
            cwd: String::new(),
            cdp: None,
        }
    }
}
