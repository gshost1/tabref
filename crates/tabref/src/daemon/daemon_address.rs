//! Where the daemon for one state root and endpoint listens. The socket lives
//! in a private folder in the user's runtime directory (`$XDG_RUNTIME_DIR`
//! on Linux, `$TMPDIR` on macOS) or else `/tmp`, named after the user id
//! and a hash of everything that shapes the daemon's behaviour: the state
//! root (`TABREF_HOME`), the `--cdp` endpoint, and the browser launch
//! settings. A runtime directory longer than 64 bytes is passed over for
//! `/tmp`, so the socket path stays below the 104 bytes macOS and the 108
//! bytes Linux allow, however long the state root is.

use std::path::{Path, PathBuf};

use tabref_cdp::CHROME_ENV;
use tabref_core::{AppError, resolve_state_root};

const SOCKET_ROOT: &str = "/tmp";
const MAX_ROOT_LEN: usize = 64;
const PREFIX: &str = "tabref-";
const CHROME_ARGS_ENV: &str = "TABREF_CHROME_ARGS";
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DaemonAddress {
    pub(crate) dir: PathBuf,
    pub(crate) state_root: PathBuf,
}

impl DaemonAddress {
    /// The address of the daemon that serves `cdp` (or the default endpoint
    /// search when `None`) for the configured state root.
    pub(crate) fn for_endpoint(cdp: Option<&str>) -> Result<Self, AppError> {
        let state_root = resolve_state_root()?;
        let fingerprint = [
            state_root.to_string_lossy().into_owned(),
            cdp.map(str::trim).unwrap_or_default().to_owned(),
            std::env::var(CHROME_ARGS_ENV).unwrap_or_default(),
            std::env::var(CHROME_ENV).unwrap_or_default(),
        ]
        .join("\n");
        Ok(Self::from_parts(
            &socket_root(),
            user_id(),
            &fingerprint,
            state_root,
        ))
    }

    pub(crate) fn from_parts(
        root: &Path,
        uid: u32,
        fingerprint: &str,
        state_root: PathBuf,
    ) -> Self {
        let name = format!("{PREFIX}{uid}-{:016x}", fnv1a(fingerprint.as_bytes()));
        Self {
            dir: root.join(name),
            state_root,
        }
    }

    /// The name prefix every daemon folder of user `uid` starts with.
    pub(crate) fn user_prefix(uid: u32) -> String {
        format!("{PREFIX}{uid}-")
    }

    pub(crate) fn socket(&self) -> PathBuf {
        self.dir.join("sock")
    }

    pub(crate) fn lock_file(&self) -> PathBuf {
        self.dir.join("lock")
    }
}

/// The folder daemon folders go in: the user's runtime directory when it is
/// an absolute, existing folder with a short enough path, else `/tmp`.
pub(crate) fn socket_root() -> PathBuf {
    let runtime = if cfg!(target_os = "macos") {
        "TMPDIR"
    } else {
        "XDG_RUNTIME_DIR"
    };
    std::env::var_os(runtime)
        .map(PathBuf::from)
        .filter(|root| root.is_absolute() && root.is_dir())
        .filter(|root| root.as_os_str().len() <= MAX_ROOT_LEN)
        .unwrap_or_else(|| PathBuf::from(SOCKET_ROOT))
}

/// The legacy and fallback folder for daemon folders.
pub(crate) fn shared_root() -> PathBuf {
    PathBuf::from(SOCKET_ROOT)
}

#[cfg(unix)]
fn user_id() -> u32 {
    super::private_dir::effective_uid()
}

#[cfg(not(unix))]
fn user_id() -> u32 {
    0
}

/// FNV-1a: stable across Rust releases, unlike the standard hasher, so a
/// new binary finds the socket of the daemon an older one started.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

#[cfg(test)]
#[path = "daemon_address_tests.rs"]
mod tests;
