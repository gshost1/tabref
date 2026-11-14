//! Where the daemon for one state root and endpoint listens. The socket lives
//! in a private folder directly under `/tmp`, named after a hash of
//! everything that shapes the daemon's behaviour: the state root
//! (`TABREF_HOME`), the `--cdp` endpoint, and the browser launch settings.
//! A short fixed prefix keeps the socket path far below the 104 bytes macOS
//! and the 108 bytes Linux allow, however long the state root is.

use std::path::{Path, PathBuf};

use tabref_cdp::CHROME_ENV;
use tabref_core::{AppError, resolve_state_root};

const SOCKET_ROOT: &str = "/tmp";
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
        Ok(Self::from_fingerprint(&fingerprint, state_root))
    }

    pub(crate) fn from_fingerprint(fingerprint: &str, state_root: PathBuf) -> Self {
        let name = format!("{PREFIX}{:016x}", fnv1a(fingerprint.as_bytes()));
        Self {
            dir: Path::new(SOCKET_ROOT).join(name),
            state_root,
        }
    }

    pub(crate) fn socket(&self) -> PathBuf {
        self.dir.join("sock")
    }

    pub(crate) fn lock_file(&self) -> PathBuf {
        self.dir.join("lock")
    }
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
