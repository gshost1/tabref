//! Daemon folders outlive their daemon: only the socket goes when a daemon
//! exits, and a folder for a state root or endpoint that is never used again
//! stays. A starting daemon removes this user's folders that no daemon holds
//! and that nobody touched for an hour, in its own root and in `/tmp`, where
//! older builds kept them.

use std::fs::OpenOptions;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::daemon_address::{DaemonAddress, shared_root, socket_root};
use super::private_dir::effective_uid;

const STALE_AFTER: Duration = Duration::from_secs(3_600);
const LEGACY_NAME_LEN: usize = "tabref-".len() + 16;

pub(crate) fn sweep_stale_dirs(own: &DaemonAddress) {
    let uid = effective_uid();
    let prefix = DaemonAddress::user_prefix(uid);
    let mut roots = vec![socket_root()];
    if roots[0] != shared_root() {
        roots.push(shared_root());
    }
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let legacy = name.len() == LEGACY_NAME_LEN
                && name
                    .strip_prefix("tabref-")
                    .is_some_and(|hash| hash.chars().all(|c| c.is_ascii_hexdigit()));
            let path = entry.path();
            if (name.starts_with(&prefix) || legacy) && path != own.dir && is_stale(&path, uid) {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

/// Whether `dir` is this user's real folder, untouched for an hour, whose
/// lock no daemon holds. Taking the lock here proves no daemon runs there;
/// it is released again when the file closes.
fn is_stale(dir: &Path, uid: u32) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(dir) else {
        return false;
    };
    let old = metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= STALE_AFTER);
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.uid() != uid || !old {
        return false;
    }
    let lock = dir.join("lock");
    match OpenOptions::new().write(true).open(&lock) {
        Ok(file) => file.try_lock().is_ok(),
        Err(_) => !lock.exists(),
    }
}
