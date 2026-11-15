//! Whether the browser `open --launch` started still runs, for platforms
//! where the owner of its DevTools port cannot be checked (and on Linux when
//! the socket tables cannot be read). A live browser keeps its port, so a
//! browser that still runs means the port is still its own. A pid alone is
//! not enough, as pids are reused: the process must also have the start time
//! recorded at launch (Linux: `/proc/<pid>/stat`; macOS: `ps -o lstart=`).
//! Windows has neither without unsafe code, so there the profile's
//! `lockfile`, which a running Chrome holds open exclusively, says it.

use std::path::Path;

use crate::LaunchedBrowser;

/// A text that names when process `pid` started, equal for the same process
/// and different for a later one with the same pid; `None` when there is no
/// such process or the platform cannot say.
pub fn process_start(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        linux_start_time(&stat)
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (output.status.success() && !text.is_empty()).then_some(text)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

/// Whether the launched browser still runs: `Some(true)` only when that
/// very process is alive; `None` when the record cannot tell (a record from
/// before start times were kept, on a platform that needs one).
pub fn launched_alive(record: &LaunchedBrowser) -> Option<bool> {
    if cfg!(windows) {
        return Some(profile_locked(&record.profile_dir));
    }
    let started = record.started.as_deref()?;
    Some(process_start(record.pid).as_deref() == Some(started))
}

/// The start time field of a `/proc/<pid>/stat` line, read after the
/// command name, which may itself hold spaces and parentheses.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn linux_start_time(stat: &str) -> Option<String> {
    let after_name = &stat[stat.rfind(')')? + 1..];
    after_name.split_whitespace().nth(19).map(str::to_owned)
}

fn profile_locked(profile_dir: &Path) -> bool {
    const SHARING_VIOLATION: i32 = 32;
    match std::fs::OpenOptions::new()
        .write(true)
        .open(profile_dir.join("lockfile"))
    {
        Ok(_) => false,
        Err(error) => error.raw_os_error() == Some(SHARING_VIOLATION),
    }
}

#[cfg(test)]
#[path = "process_start_tests.rs"]
mod tests;
