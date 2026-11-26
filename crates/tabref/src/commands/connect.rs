//! `tabref connect --user-chrome`: the explicit opt-in to driving the user's
//! own, logged-in browser, and `tabref connect --off` to stop. The opt-in is
//! changed in the client, never in the daemon. A change either way stops
//! this state directory's daemon (the one without an endpoint, which is the
//! one that uses the user's browser), cancelling what it runs and waiting for
//! its process to exit, so no connection made before the change stays open.
//! The first connection after an opt-in is then made by the daemon, so the
//! one the user approves in the browser's prompt is the one later commands
//! keep using; a repeated opt-in to the same browser keeps the daemon and
//! the connection (or the prompt) it holds. A command
//! running elsewhere (another shell, `TABREF_NO_DAEMON`) stops at its next
//! request, as its connection checks the opt-in.

use std::path::Path;

use serde_json::json;
use tabref_core::{AppError, RefStore};

use crate::cli::Cli;
use crate::daemon::control::stop_endpoint_less;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::user_chrome::{find_user_chrome, shown_dir};
use crate::session::user_chrome_record::{forget_user_chrome, load_user_chrome, save_user_chrome};

pub(crate) fn run(
    cli: &Cli,
    user_chrome: bool,
    user_data_dir: Option<&Path>,
    off: bool,
) -> Result<Outcome, AppError> {
    let store = RefStore::open()?;
    if off {
        return turn_off(&store);
    }
    if !user_chrome {
        return Err(AppError::invalid_args(
            "Say which: 'tabref connect --user-chrome' to drive your own logged-in browser \
             (opt-in), or 'tabref connect --off'",
        ));
    }
    if cli.cdp.is_some() {
        return Err(AppError::invalid_args(
            "--cdp (or TABREF_CDP) names another endpoint, which every command would use instead \
             of your browser; unset it to use 'connect --user-chrome'",
        ));
    }
    let record = find_user_chrome(user_data_dir)?;
    let again = load_user_chrome(&store).as_ref() == Some(&record);
    if !again {
        stop_endpoint_less()?;
        save_user_chrome(&store, &record)?;
    }
    let attached = crate::daemon::run_through_daemon(cli).unwrap_or_else(|| {
        let mut session = Session::open(&crate::commands::session_options(cli))?;
        let outcome = report(&mut session);
        session.with_warning(outcome)
    });
    match attached {
        Err(mut error) if kept_waiting(&error) => {
            if let Some(suggestion) = error.suggestion.as_mut() {
                suggestion.push_str(
                    ". The opt-in is kept while the prompt waits; 'tabref connect --off' drops it",
                );
            }
            Err(error)
        }
        Err(error) if again && !declined(&error) => Err(error),
        Err(error) => {
            forget_user_chrome(&store)?;
            stop_endpoint_less()?;
            Err(error)
        }
        Ok(outcome) => Ok(outcome),
    }
}

/// Whether the user's browser refused the connection (its prompt answered
/// with Cancel), which ends even an opt-in made before this `connect`; a
/// passing failure (a busy turn, a lost reply) leaves that one as it was.
fn declined(error: &AppError) -> bool {
    error.kind() == Some("cdp_connection_rejected")
}

/// Whether `error` is a connection to the user's browser that the daemon
/// keeps waiting for the user's approval.
fn kept_waiting(error: &AppError) -> bool {
    error.kind() == Some("cdp_approval_pending")
        && error
            .details
            .as_ref()
            .is_some_and(|details| details["prompt_kept"] == json!(true))
}

/// What `connect --user-chrome` reports once `session` reached the user's
/// browser: run in the daemon, so the approved connection is the one later
/// commands use, or in-process when no daemon can run.
pub(crate) fn report(session: &mut Session) -> Result<Outcome, AppError> {
    let record = load_user_chrome(session.store()).ok_or_else(|| {
        AppError::invalid_args("The opt-in to your browser was removed while connecting")
    })?;
    let deadline = session.deadline();
    let product = session
        .connection()
        .call("Browser.getVersion", json!({}), None, deadline)
        .ok()
        .and_then(|version| version["product"].as_str().map(str::to_owned));
    let text = format!(
        "attached to your {}{} (profile {}, pid {}): later commands drive this logged-in browser \
         until 'tabref connect --off'",
        record.name,
        product
            .as_deref()
            .map_or_else(String::new, |product| format!(", {product}")),
        shown_dir(&record.user_data_dir),
        record.pid,
    );
    let data = json!({
        "attached": true,
        "browser": record.name,
        "product": product,
        "user_data_dir": record.user_data_dir,
        "pid": record.pid,
        "port": session.endpoint().port(),
    });
    Ok(Outcome::new(text, data))
}

/// Removes the opt-in (confirmed gone), then stops this state directory's
/// daemon and waits for its process to exit. It says only what it checked.
fn turn_off(store: &RefStore) -> Result<Outcome, AppError> {
    let was = load_user_chrome(store);
    forget_user_chrome(store)?;
    let stopped = stop_endpoint_less()?;
    let opt_in = match &was {
        Some(record) => format!("stopped using your {}: the opt-in is removed", record.name),
        None => "tabref was not using your browser".to_owned(),
    };
    let daemon = match stopped {
        Some(pid) => {
            format!("this state directory's daemon (pid {pid}) was stopped and has exited")
        }
        None => "this state directory's daemon was not running".to_owned(),
    };
    Ok(Outcome::new(
        format!("{opt_in}; {daemon}"),
        json!({ "attached": false, "stopped_daemon": stopped }),
    ))
}
