//! `tabref close --browser`: quits the browser `open --launch` started, as
//! this state directory's launch record names it (pid and start time), and
//! never any other. It runs in the client: it first stops this state
//! directory's daemon, so no kept connection outlives the browser. Then it
//! asks the browser to close (`Browser.close`, so a profile writes out what
//! it holds), kills its process group if it is still there, removes a
//! throwaway profile (never a `--profile` one) and forgets the record.

use std::time::Duration;

use serde_json::json;
use tabref_cdp::{LaunchedBrowser, abandon_launch, connect, connect_held, launched_alive};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, RefStore};

use crate::cli::Cli;
use crate::daemon::control::stop_endpoint_less;
use crate::output::outcome::Outcome;
use crate::session::endpoint_choice::launched_check;
use crate::session::launch_record::{forget_launched, load_launched};

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
    let Some(record) = load_launched(&store) else {
        return Err(AppError::new(
            ErrorCode::ActionFailed,
            "No browser started by 'tabref open --launch' is recorded in this state directory, \
             and tabref never quits a browser it did not start",
        )
        .with_suggestion("Quit that browser yourself; 'tabref close' closes a tab")
        .with_details(json!({ "kind": "no_launched_browser" }))
        .with_disposition(DeliverySemantics::not_delivered()));
    };
    let Some(was_running) = launched_alive(&record) else {
        return Err(refused(
            "the launch record does not say when its browser started, so tabref cannot tell it \
             from another program with the same pid",
        ));
    };
    let stopped_daemon = stop_endpoint_less()?;
    let deadline = Deadline::after(cli.timeout_ms())?.capped(CLOSE_BUDGET);
    let graceful = was_running && ask_to_close(&record, deadline) && exited(&record, deadline);
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
    forget_launched(&store);
    Ok(closed(&record, was_running, graceful, stopped_daemon))
}

/// Sends `Browser.close` when the record's port is still its browser's.
fn ask_to_close(record: &LaunchedBrowser, deadline: Deadline) -> bool {
    let Ok(endpoint) = record.endpoint() else {
        return false;
    };
    let connection = match launched_check(record) {
        Some(Some(held)) => connect_held(&endpoint, &held, deadline),
        Some(None) => connect(&endpoint, deadline),
        None => return false,
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
    let profile_removed = !record.persistent && !record.profile_dir.exists();
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
    let profile = if record.persistent {
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
            "profile_kept": record.persistent,
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
