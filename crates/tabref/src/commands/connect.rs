//! `tabref connect --user-chrome`: the explicit opt-in to driving the user's
//! own, logged-in browser, and `tabref connect --off` to stop. It runs in the
//! client, never in the daemon. Either way it stops this state directory's
//! daemon (the one without an endpoint, which is the one that uses the
//! user's browser), cancelling what it runs and waiting for its process to
//! exit, so no connection made before the change stays open. A command
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
    stop_endpoint_less()?;
    save_user_chrome(&store, &record)?;
    let mut session = match Session::open(&crate::commands::session_options(cli)) {
        Ok(session) => session,
        Err(error) => {
            forget_user_chrome(&store)?;
            return Err(error);
        }
    };
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
    session.with_warning(Ok(Outcome::new(text, data)))
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
