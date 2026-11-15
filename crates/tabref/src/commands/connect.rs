//! `tabref connect --user-chrome`: the explicit opt-in to driving the user's
//! own, logged-in browser, and `tabref connect --off` to stop. It runs in the
//! client, never in the daemon: it records the choice, and a daemon
//! connection made before the choice changed is dropped on its next command
//! (see `kept_session`).

use std::path::Path;

use serde_json::json;
use tabref_cdp::profile_lock_pid;
use tabref_core::{AppError, RefStore};

use crate::cli::Cli;
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
        let was = load_user_chrome(&store);
        forget_user_chrome(&store);
        let text = match &was {
            Some(record) => format!(
                "stopped using your {}; later commands no longer touch it",
                record.name
            ),
            None => "tabref was not using your browser".to_owned(),
        };
        return Ok(Outcome::new(text, json!({ "attached": false })));
    }
    if !user_chrome {
        return Err(AppError::invalid_args(
            "Say which: 'tabref connect --user-chrome' to drive your own logged-in browser \
             (opt-in), or 'tabref connect --off'",
        ));
    }
    if cli.cdp.is_some() {
        return Err(AppError::invalid_args(
            "--cdp names an endpoint already; 'connect --user-chrome' finds your own browser",
        ));
    }
    let record = find_user_chrome(user_data_dir)?;
    save_user_chrome(&store, &record)?;
    let mut session = match Session::open(&crate::commands::session_options(cli)) {
        Ok(session) => session,
        Err(error) => {
            forget_user_chrome(&store);
            return Err(error);
        }
    };
    let deadline = session.deadline();
    let product = session
        .connection()
        .call("Browser.getVersion", json!({}), None, deadline)
        .ok()
        .and_then(|version| version["product"].as_str().map(str::to_owned));
    let port = session.endpoint().port();
    let pid = profile_lock_pid(&record.user_data_dir);
    let shown = shown_dir(&record.user_data_dir);
    let text = format!(
        "attached to your {}{} (profile {shown}{}): later commands drive this logged-in browser \
         until 'tabref connect --off'",
        record.name,
        product
            .as_deref()
            .map_or_else(String::new, |product| format!(", {product}")),
        pid.map_or_else(String::new, |pid| format!(", pid {pid}")),
    );
    let data = json!({
        "attached": true,
        "browser": record.name,
        "product": product,
        "user_data_dir": record.user_data_dir,
        "pid": pid,
        "port": port,
    });
    session.with_warning(Ok(Outcome::new(text, data)))
}
