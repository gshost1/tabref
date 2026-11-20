//! Ending a browser tabref started: one that could not be used (its
//! DevTools never answered), so a failed launch leaves no browser running
//! and no throwaway profile behind, and one `close --browser` quits. The
//! kill goes only to the browser the record names, checked as
//! `recorded_process` says (never an adopted one, never on Windows). A
//! profile is deleted only when it is one `launch_chrome` made: a real
//! directory of this user, right in the temp folder, with the generated
//! name, so a record naming any other folder never has it deleted.

use std::path::Path;
use std::time::{Duration, Instant};

use super::process_start::launched_alive;
use super::recorded_process::kill_recorded;
use crate::LaunchedBrowser;

/// The start of a throwaway profile's directory name; `s` and seven
/// base-36 digits follow (`new_snapshot_id`).
pub(crate) const THROWAWAY_PREFIX: &str = "tabref-profile-";
const ID_LEN: usize = 8;
const GRACE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(25);

/// Kills `browser` and its process group when it still is the browser the
/// record names (see `recorded_process`), waits for it to go, and then
/// removes its profile when that is a throwaway one tabref made. Returns
/// whether the browser is gone.
pub fn abandon_launch(browser: &LaunchedBrowser) -> bool {
    if launched_alive(browser) == Some(true) {
        kill_recorded(browser);
    }
    let ended = wait_gone(browser);
    if ended && !browser.profile_use.persistent {
        remove_profile_soon(&browser.profile_dir);
    }
    ended
}

/// Removes the throwaway `profile_dir` of a launch that failed or a browser
/// that quit, retrying while a browser that is going still writes into it.
pub(crate) fn remove_profile_soon(profile_dir: &Path) {
    let started = Instant::now();
    if !is_throwaway(profile_dir) {
        return;
    }
    let _ = std::fs::remove_dir_all(profile_dir);
    while profile_dir.exists() && started.elapsed() < GRACE {
        std::thread::sleep(POLL);
        let _ = std::fs::remove_dir_all(profile_dir);
    }
}

fn wait_gone(browser: &LaunchedBrowser) -> bool {
    let started = Instant::now();
    loop {
        match launched_alive(browser) {
            Some(false) => return true,
            None => return false,
            Some(true) if started.elapsed() >= GRACE => return false,
            Some(true) => std::thread::sleep(POLL),
        }
    }
}

/// Whether `profile_dir` is a throwaway profile `launch_chrome` made: its
/// name is the generated one, its folder is the temp folder, and it is a
/// directory (not a link) of this user.
fn is_throwaway(profile_dir: &Path) -> bool {
    let generated = profile_dir
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(THROWAWAY_PREFIX))
        .is_some_and(|id| {
            id.len() == ID_LEN
                && id.starts_with('s')
                && id[1..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
        });
    let in_temp = match (
        profile_dir.parent().map(std::fs::canonicalize),
        std::fs::canonicalize(std::env::temp_dir()),
    ) {
        (Some(Ok(parent)), Ok(temp)) => parent == temp,
        _ => false,
    };
    generated && in_temp && is_own_directory(profile_dir)
}

fn is_own_directory(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.uid() == rustix::process::geteuid().as_raw()
    }
    #[cfg(windows)]
    {
        crate::discovery::windows_profile_owner::is_own_directory(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

/// Kills the process group `launch_chrome`'s own child leads, while that
/// child is not yet reaped (so its pid cannot have been reused). Windows
/// has no group kill here; the caller kills the child itself.
#[cfg(unix)]
pub(crate) fn kill_child_group(pid: u32) {
    use rustix::process::{Pid, Signal, kill_process_group};
    if let Some(pid) = i32::try_from(pid).ok().and_then(Pid::from_raw) {
        let _ = kill_process_group(pid, Signal::KILL);
    }
}

#[cfg(not(unix))]
pub(crate) fn kill_child_group(_pid: u32) {}

#[cfg(test)]
#[path = "abandon_tests.rs"]
mod tests;
