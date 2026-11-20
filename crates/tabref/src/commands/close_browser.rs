//! `tabref close --browser`: quits the browser `open --launch` started, as
//! this state directory's launch record names it, and never any other. It
//! runs in the client: it first stops this state directory's daemon, so no
//! kept connection outlives the browser, then holds the launch-record lock
//! for the rest, so no launch saves a record in between. It refuses when the
//! recorded pid is not that browser any more (another program, or another
//! profile). It asks the browser to close (`Browser.close`, so a profile
//! writes out what it holds). A browser it started itself that is still
//! there is then killed with its process group (`recorded_process`); one it
//! adopted, or any on Windows, is only asked. Then it removes a throwaway
//! profile (never a `--profile` one) and forgets the record, if the record
//! still is the one it closed.

use std::time::Duration;

use serde_json::json;
use tabref_cdp::{
    LaunchedBrowser, abandon_launch, connect, connect_held, is_launched_browser, launched_alive,
};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, RefStore};

use crate::cli::Cli;
use crate::daemon::control::stop_endpoint_less;
use crate::output::outcome::Outcome;
use crate::session::launch_check::{LaunchCheck, launched_check};
use crate::session::launch_record::{forget_launched, load_launched, lock_launched, read_launched};

const CLOSE_BUDGET: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(25);

pub(crate) fn run(cli: &Cli) -> Result<Outcome, AppError> {
    if cli.cdp.is_some() {
        return Err(refused(
            "--cdp (or TABREF_CDP) names a browser tabref did not start; 'close --browser' quits \
             only the one 'open --launch' started",
        ));
    }
    let store = RefStore::open()?;
    if read_launched(&store).is_none() {
        return Err(no_launched_browser());
    }
    let stopped_daemon = stop_endpoint_less()?;
    let _turn = lock_launched(&store)?;
    let Some(record) = load_launched(&store) else {
        return Err(no_launched_browser());
    };
    let Some(was_running) = launched_alive(&record) else {
        return Err(refused(
            "the launch record does not say when its browser started, so tabref cannot tell it \
             from another program with the same pid",
        ));
    };
    if was_running && is_launched_browser(&record) == Some(false) {
        return Err(refused(&format!(
            "pid {} is not the browser 'open --launch' started on {} (another program, or \
             another profile); nothing was signalled or deleted",
            record.pid,
            record.profile_dir.display()
        )));
    }
    let deadline = Deadline::after(cli.timeout_ms())?.capped(CLOSE_BUDGET);
    let graceful = was_running && ask_to_close(&record, deadline) && exited(&record, deadline);
    let may_kill = cfg!(unix) && !record.profile_use.adopted;
    if was_running && !graceful && !may_kill {
        return Err(not_closed(&record));
    }
    if !abandon_launch(&record) {
        return Err(AppError::new(
            ErrorCode::ActionFailed,
            format!(
                "The browser 'open --launch' started (pid {}) did not exit after it was killed",
                record.pid
            ),
        )
        .with_details(json!({ "kind": "browser_did_not_exit", "pid": record.pid })));
    }
    if load_launched(&store).as_ref() == Some(&record) {
        forget_launched(&store);
    }
    Ok(closed(&record, was_running, graceful, stopped_daemon))
}

fn no_launched_browser() -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        "No browser started by 'tabref open --launch' is recorded in this state directory, and \
         tabref never quits a browser it did not start",
    )
    .with_suggestion("Quit that browser yourself; 'tabref close' closes a tab")
    .with_details(json!({ "kind": "no_launched_browser" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// The browser was asked to close and did not, and tabref may not kill it:
/// it adopted it (another state directory started it), or this is Windows.
fn not_closed(record: &LaunchedBrowser) -> AppError {
    let why = if record.profile_use.adopted {
        "another state directory started it, so tabref only asks it to close"
    } else {
        "on Windows tabref only asks a browser to close and never kills it"
    };
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "The browser on {} (pid {}) did not close when asked; {why}",
            record.profile_dir.display(),
            record.pid
        ),
    )
    .with_suggestion("Close that browser yourself, or retry with a longer --timeout")
    .with_details(json!({ "kind": "browser_not_closed", "pid": record.pid }))
}

/// Sends `Browser.close` when the record's port is still its browser's.
fn ask_to_close(record: &LaunchedBrowser, deadline: Deadline) -> bool {
    let Ok(endpoint) = record.endpoint() else {
        return false;
    };
    let connection = match launched_check(record) {
        LaunchCheck::Held(held) => connect_held(&endpoint, &held, deadline),
        LaunchCheck::Alive => connect(&endpoint, deadline),
        LaunchCheck::Unverified(_) | LaunchCheck::Gone => return false,
    };
    connection.is_ok_and(|mut connection| {
        connection
            .call("Browser.close", json!({}), None, deadline)
            .is_ok()
    })
}

/// Whether the browser exited (after it was asked to close) by `deadline`.
fn exited(record: &LaunchedBrowser, deadline: Deadline) -> bool {
    while launched_alive(record) == Some(true) && !deadline.is_expired() {
        std::thread::sleep(POLL.min(deadline.remaining()));
    }
    launched_alive(record) == Some(false)
}

fn closed(
    record: &LaunchedBrowser,
    was_running: bool,
    graceful: bool,
    stopped_daemon: Option<u32>,
) -> Outcome {
    let profile_removed = !record.profile_use.persistent && !record.profile_dir.exists();
    let how = match (was_running, graceful) {
        (false, _) => format!(
            "the browser 'open --launch' started (pid {}) was not running",
            record.pid
        ),
        (true, true) => format!(
            "closed the browser 'open --launch' started (pid {})",
            record.pid
        ),
        (true, false) => format!(
            "killed the browser 'open --launch' started (pid {}), which did not close when asked",
            record.pid
        ),
    };
    let profile = if record.profile_use.persistent {
        format!("kept its profile {}", record.profile_dir.display())
    } else if profile_removed {
        "removed its throwaway profile".to_owned()
    } else {
        format!(
            "its throwaway profile {} is still there",
            record.profile_dir.display()
        )
    };
    Outcome::new(
        format!("{how}; {profile}"),
        json!({
            "closed": true,
            "was_running": was_running,
            "graceful": graceful,
            "pid": record.pid,
            "profile_dir": record.profile_dir,
            "profile_kept": record.profile_use.persistent,
            "profile_removed": profile_removed,
            "stopped_daemon": stopped_daemon,
        }),
    )
}

fn refused(reason: &str) -> AppError {
    AppError::new(ErrorCode::InvalidArgs, format!("Not quitting: {reason}"))
        .with_details(json!({ "kind": "close_browser_refused" }))
        .with_disposition(DeliverySemantics::not_delivered())
}
