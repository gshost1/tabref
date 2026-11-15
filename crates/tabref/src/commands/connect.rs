//! `tabref connect --user-chrome`: the explicit opt-in to driving the user's
//! own, logged-in browser, and `tabref connect --off` to stop. It runs in the
//! client, never in the daemon. Either way it stops the daemon that serves
//! this state directory, so no connection made before the change stays open:
//! `--off` confirms that the record is gone and that no daemon still runs.

use std::path::Path;

use serde_json::json;
use tabref_core::{AppError, RefStore};

use crate::cli::Cli;
use crate::daemon::control;
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
    if cli.cdp.is_some() {
        return Err(AppError::invalid_args(
            "'connect' takes no --cdp: it chooses whether tabref uses your own browser",
        ));
    }
    let store = RefStore::open()?;
    if off {
        return turn_off(cli, &store);
    }
    if !user_chrome {
        return Err(AppError::invalid_args(
            "Say which: 'tabref connect --user-chrome' to drive your own logged-in browser \
             (opt-in), or 'tabref connect --off'",
        ));
    }
    let record = find_user_chrome(user_data_dir)?;
    stop_daemon(cli)?;
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

/// Removes the opt-in, stops the daemon that may hold a connection to the
/// user's browser, and confirms both.
fn turn_off(cli: &Cli, store: &RefStore) -> Result<Outcome, AppError> {
    let was = load_user_chrome(store);
    forget_user_chrome(store)?;
    stop_daemon(cli)?;
    let text = match &was {
        Some(record) => format!(
            "stopped using your {}: the opt-in is removed and no tabref daemon holds a connection \
             to it",
            record.name
        ),
        None => "tabref was not using your browser; no tabref daemon is running".to_owned(),
    };
    Ok(Outcome::new(
        text,
        json!({ "attached": false, "daemon_running": false }),
    ))
}

/// Stops this state directory's daemon and confirms it no longer runs.
fn stop_daemon(cli: &Cli) -> Result<(), AppError> {
    control::run(cli, "stop")?;
    let status = control::run(cli, "status")?;
    if status.data["running"] == json!(true) {
        return Err(AppError::internal(
            "The tabref daemon did not stop, so it may still hold a connection to your browser; \
             run 'tabref daemon stop'",
        ));
    }
    Ok(())
}
