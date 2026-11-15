//! Which process runs a profile. A running Chrome-family browser keeps
//! `<user-data-dir>/SingletonLock`, a symlink to `<host>-<browser pid>`, so
//! the lock names the one process that may own the profile's DevTools port.
//! Only the link is read, never the profile's contents. Windows keeps no
//! such link, so there it is `None`.

use std::path::Path;

/// The pid of the browser process that holds `user_data_dir`'s profile lock.
pub fn profile_lock_pid(user_data_dir: &Path) -> Option<u32> {
    let target = std::fs::read_link(user_data_dir.join("SingletonLock")).ok()?;
    lock_pid(&target.to_string_lossy())
}

pub(crate) fn lock_pid(target: &str) -> Option<u32> {
    target.rsplit_once('-')?.1.parse().ok()
}

#[cfg(test)]
#[path = "profile_lock_tests.rs"]
mod tests;
