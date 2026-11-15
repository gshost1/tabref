//! Ending a browser tabref started that could not be used (its DevTools
//! never answered), so a failed launch leaves no browser running and no
//! throwaway profile behind. Only for a browser this process launched: an
//! adopted one, or the user's, is never ended here.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::process_start::launched_alive;
use crate::LaunchedBrowser;

const GRACE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(25);

/// Kills the process group of `browser` (it runs in its own, see
/// `launch_chrome`) if its process is still the one launched, waits for it
/// to go, and then removes its profile unless the profile is persistent.
/// Returns whether the browser is gone.
pub fn abandon_launch(browser: &LaunchedBrowser) -> bool {
    if launched_alive(browser) != Some(false) {
        kill_group(browser.pid);
    }
    let ended = wait_gone(browser);
    if ended && !browser.persistent {
        remove_profile_soon(&browser.profile_dir);
    }
    ended
}

/// Removes the throwaway `profile_dir` of a launch that failed, retrying
/// while a browser that is going still writes into it.
pub(crate) fn remove_profile_soon(profile_dir: &Path) {
    let started = Instant::now();
    while profile_dir.exists() && started.elapsed() < GRACE {
        remove_throwaway(profile_dir);
        if profile_dir.exists() {
            std::thread::sleep(POLL);
        }
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

fn remove_throwaway(profile_dir: &Path) {
    let _ = std::fs::remove_dir_all(profile_dir);
}

#[cfg(unix)]
pub(crate) fn kill_group(pid: u32) {
    let _ = Command::new("/bin/kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(windows)]
pub(crate) fn kill_group(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn kill_group(_pid: u32) {}
