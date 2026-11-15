use std::collections::BTreeSet;
use std::path::PathBuf;

use tabref_core::AppError;

use super::connect_error::port_changed_hands;
use super::listener_owner::listening_inodes;

/// A port checked to be held by trusted processes, with the listening
/// sockets the check saw, so a connection can confirm, after its TCP connect
/// and before it sends anything, that the port did not change hands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldPort {
    pub port: u16,
    pub(crate) inodes: BTreeSet<u64>,
    pub(crate) proc_root: PathBuf,
}

impl HeldPort {
    /// `Ok` while the same sockets listen on the port; otherwise kind
    /// `cdp_endpoint_changed`.
    pub fn confirm(&self) -> Result<(), AppError> {
        match listening_inodes(&self.proc_root, self.port) {
            Ok(now) if now == self.inodes => Ok(()),
            _ => Err(port_changed_hands(self.port)),
        }
    }
}
