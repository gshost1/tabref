//! Whether the browser a launch record names still holds its port, decided
//! before anything is sent to that port. Where the port's owner can be
//! checked (Linux), a process running the record's profile must hold it.
//! Elsewhere that very process must still run (pid and start time; Windows:
//! the profile's lock), so a reused pid is not taken for it. Either way the
//! profile's `DevToolsActivePort` must still name the record's port and
//! browser run (its DevTools id), so a browser started later on the profile,
//! or a record pointed at another browser, is not taken for it.

use std::path::Path;

use tabref_cdp::{
    ActivePort, HeldPort, LaunchedBrowser, ListenerVerdict, ProcessView, check_listener,
    launched_alive, read_active_port,
};
use tabref_core::AppError;

/// What [`launched_check`] found.
#[derive(Debug)]
pub(crate) enum LaunchCheck {
    /// A process running the record's profile holds the port.
    Held(HeldPort),
    /// The port's owner cannot be checked, but the recorded process still
    /// runs.
    Alive,
    /// The browser is gone, its port is free, or something else holds it.
    Gone,
    /// The browser may still run, but its profile does not confirm the
    /// record (kind `cdp_launch_unverified`).
    Unverified(AppError),
}

impl LaunchCheck {
    /// Whether the record's browser may be taken to hold its port.
    pub(crate) fn is_launched(&self) -> bool {
        matches!(self, Self::Held(_) | Self::Alive)
    }
}

/// [`decide`] for this machine.
pub(crate) fn launched_check(record: &LaunchedBrowser) -> LaunchCheck {
    decide(
        record,
        check_listener(record.port, &launched_from(&record.profile_dir)),
        &|| launched_alive(record),
        &|| read_active_port(&record.profile_dir),
    )
}

/// The launch check from the port's `verdict`, whether the recorded process
/// is `alive` (asked only when the verdict cannot say), and the profile's
/// `DevToolsActivePort` as `read`.
pub(crate) fn decide(
    record: &LaunchedBrowser,
    verdict: ListenerVerdict,
    alive: &dyn Fn() -> Option<bool>,
    read: &dyn Fn() -> Result<ActivePort, AppError>,
) -> LaunchCheck {
    let _ = read;
    match verdict {
        ListenerVerdict::Trusted(held) => LaunchCheck::Held(held),
        ListenerVerdict::Nothing | ListenerVerdict::Untrusted { .. } => LaunchCheck::Gone,
        ListenerVerdict::Unknown | ListenerVerdict::CantCheck(_) => {
            if alive() == Some(true) {
                LaunchCheck::Alive
            } else {
                LaunchCheck::Gone
            }
        }
    }
    .with_record(record)
}

impl LaunchCheck {
    fn with_record(self, _record: &LaunchedBrowser) -> Self {
        self
    }
}

/// Whether a process runs the browser `open --launch` started with
/// `profile_dir`, as its command line names that profile.
pub(crate) fn launched_from(profile_dir: &Path) -> impl Fn(&ProcessView) -> bool + use<> {
    let flag = format!("--user-data-dir={}", profile_dir.display());
    move |process: &ProcessView| process.args().contains(&flag)
}

#[cfg(test)]
#[path = "launch_check_tests.rs"]
mod tests;
