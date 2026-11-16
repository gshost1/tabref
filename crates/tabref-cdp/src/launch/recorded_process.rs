//! Killing the browser a launch record names, and nothing else. A record is
//! a file anyone who can write the state directory could have written, so
//! its pid is trusted only when that process still is the browser tabref
//! launched: the same start time (and, on Linux, the same boot), a
//! Chrome-family binary running the recorded `--user-data-dir`, and the
//! leader of its own process group (as `launch_chrome` starts it). Never pid
//! 0 or 1, never an adopted browser, and never on Windows, where tabref has
//! no start time to check and only asks the browser to close.
//!
//! On Linux the process is pinned with a pidfd before it is checked again,
//! so the signal reaches the process that was checked even if it exits and
//! its pid is reused in between; then its group is killed too.

use crate::{LaunchedBrowser, ProcessView, is_chrome_family};

/// Whether `record`'s browser may be signalled, given the process group
/// `pgid` of its pid and what `view` says that process runs.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn may_signal(
    record: &LaunchedBrowser,
    pgid: Option<u32>,
    view: Option<&ProcessView>,
) -> bool {
    let flag = format!("--user-data-dir={}", record.profile_dir.display());
    record.pid > 1
        && !record.profile_use.adopted
        && pgid == Some(record.pid)
        && view.is_some_and(|view| {
            view.pid == record.pid && is_chrome_family(view) && view.args().contains(&flag)
        })
}

/// Whether `record`'s pid still is the browser it names, as [`may_signal`]
/// checks it but for an adopted browser too: the same process (start
/// time), a Chrome-family binary on the recorded profile, leading its own
/// group. `None` where it cannot be told (Windows).
#[cfg(unix)]
pub fn is_launched_browser(record: &LaunchedBrowser) -> Option<bool> {
    let mut own = record.clone();
    own.profile_use.adopted = false;
    Some(identity_holds(&own))
}

#[cfg(not(unix))]
pub fn is_launched_browser(_record: &LaunchedBrowser) -> Option<bool> {
    None
}

/// Kills `record`'s browser and its process group when [`may_signal`] holds
/// for it and it still is the process recorded. Returns whether it sent the
/// kill.
#[cfg(unix)]
pub(crate) fn kill_recorded(record: &LaunchedBrowser) -> bool {
    use rustix::process::{Signal, kill_process_group};

    let Some(pid) = rustix_pid(record.pid) else {
        return false;
    };
    #[cfg(target_os = "linux")]
    let Ok(pinned) = rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()) else {
        return false;
    };
    if !identity_holds(record) {
        return false;
    }
    #[cfg(target_os = "linux")]
    let sent = rustix::process::pidfd_send_signal(&pinned, Signal::KILL).is_ok();
    #[cfg(not(target_os = "linux"))]
    let sent = rustix::process::kill_process(pid, Signal::KILL).is_ok();
    if sent {
        let _ = kill_process_group(pid, Signal::KILL);
    }
    sent
}

#[cfg(unix)]
fn identity_holds(record: &LaunchedBrowser) -> bool {
    let Some(pid) = rustix_pid(record.pid) else {
        return false;
    };
    let pgid = rustix::process::getpgid(Some(pid))
        .ok()
        .and_then(|pgid| u32::try_from(pgid.as_raw_pid()).ok());
    super::process_start::launched_alive(record) == Some(true)
        && may_signal(record, pgid, process_view(record.pid).as_ref())
}

#[cfg(unix)]
fn rustix_pid(pid: u32) -> Option<rustix::process::Pid> {
    i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
}

/// Windows: never; see the module documentation.
#[cfg(not(unix))]
pub(crate) fn kill_recorded(_record: &LaunchedBrowser) -> bool {
    false
}

/// What process `pid` of this user runs.
#[cfg(target_os = "linux")]
fn process_view(pid: u32) -> Option<ProcessView> {
    let uid = rustix::process::geteuid().as_raw();
    crate::discovery::listener_owner::own_process(
        &std::path::Path::new("/proc").join(pid.to_string()),
        uid,
    )
}

/// What process `pid` runs, from `ps` (the binary's path, and the command
/// line as one string, which `ProcessView::args` splits).
#[cfg(all(unix, not(target_os = "linux")))]
fn process_view(pid: u32) -> Option<ProcessView> {
    let field = |name: &str| {
        let output = std::process::Command::new("/bin/ps")
            .args(["-o", name, "-p", &pid.to_string()])
            .env("LC_ALL", "C")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (output.status.success() && !text.is_empty()).then_some(text)
    };
    Some(ProcessView {
        pid,
        exe: std::path::PathBuf::from(field("comm=")?),
        cmdline: vec![field("args=")?],
    })
}

#[cfg(test)]
#[path = "recorded_process_tests.rs"]
mod tests;
