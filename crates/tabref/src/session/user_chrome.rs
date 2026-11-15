//! Attaching to the user's own, logged-in browser: opt-in only, through
//! `tabref connect --user-chrome`. The browser's `DevToolsActivePort` (written
//! when its user turns remote debugging on at chrome://inspect) gives the
//! port. Before anything is sent there, the port must be held by the very
//! process that holds that profile's lock (Linux), so a stale file or a
//! program that took the port over is never attached to. Elsewhere the
//! owner cannot be checked cheaply: the command says so in a warning, and the
//! browser's own approval prompt still stands in the way.

use std::path::{Path, PathBuf};

use tabref_cdp::{
    ListenerVerdict, ProcessView, check_listener, connect, connect_held, default_user_data_dirs,
    endpoint_from_user_data_dir, is_chrome_family, profile_lock_pid, read_active_port,
};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::connected_browser::ConnectedBrowser;
use super::user_chrome_record::UserChromeRecord;

const AGAIN: &str = "Turn remote debugging on at chrome://inspect/#remote-debugging and run \
                     'tabref connect --user-chrome' again, or 'tabref connect --off' to stop using it";

/// The user's browser to attach to: the one in `dir`, or the one default
/// profile whose browser has remote debugging on. Only these directories'
/// `DevToolsActivePort` files are read.
pub(crate) fn find_user_chrome(dir: Option<&Path>) -> Result<UserChromeRecord, AppError> {
    if let Some(dir) = dir {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        read_active_port(&dir)?;
        let name = default_user_data_dirs()
            .into_iter()
            .find(|browser| browser.user_data_dir == dir)
            .map_or("Chrome", |browser| browser.name);
        return Ok(UserChromeRecord {
            name: name.to_owned(),
            user_data_dir: dir,
        });
    }
    let found: Vec<UserChromeRecord> = default_user_data_dirs()
        .into_iter()
        .filter(|browser| read_active_port(&browser.user_data_dir).is_ok())
        .map(|browser| UserChromeRecord {
            name: browser.name.to_owned(),
            user_data_dir: browser.user_data_dir,
        })
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(AppError::new(
            ErrorCode::ActionFailed,
            "None of your browsers has remote debugging turned on",
        )
        .with_suggestion(
            "Turn it on at chrome://inspect/#remote-debugging in your browser (it asks you to allow \
             each connection), then run 'tabref connect --user-chrome' again",
        )
        .with_details(serde_json::json!({ "kind": "cdp_user_chrome_not_found" }))
        .with_disposition(DeliverySemantics::not_delivered())),
        several => Err(AppError::new(
            ErrorCode::AmbiguousTarget,
            format!("{} of your browsers have remote debugging on", several.len()),
        )
        .with_suggestion(format!(
            "Name one with --user-data-dir: {}",
            several
                .iter()
                .map(|record| record.user_data_dir.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .with_details(serde_json::json!({ "kind": "cdp_user_chrome_ambiguous" }))
        .with_disposition(DeliverySemantics::not_delivered())),
    }
}

/// Connects to the user's browser `record` names, after checking who holds
/// the port its profile advertises.
pub(crate) fn connect_user_chrome(
    record: &UserChromeRecord,
    deadline: Deadline,
) -> Result<ConnectedBrowser, AppError> {
    let dir = &record.user_data_dir;
    let endpoint = endpoint_from_user_data_dir(dir).map_err(|_| gone(record))?;
    let port = endpoint.port();
    let verdict = check_listener(port, &runs_profile(profile_lock_pid(dir)));
    let (connection, warning) = match &verdict {
        ListenerVerdict::Trusted(held) => (connect_held(&endpoint, held, deadline)?, None),
        ListenerVerdict::Unknown => (
            connect(&endpoint, deadline)?,
            Some(format!(
                "who holds port {port} cannot be checked on this platform; tabref relies on your \
                 browser's approval prompt"
            )),
        ),
        ListenerVerdict::Nothing => return Err(gone(record)),
        refused => return Err(not_the_profile(record, port, refused)),
    };
    Ok(ConnectedBrowser {
        endpoint,
        connection,
        warning,
    })
}

/// Whether a process is the browser holding the profile lock (`lock_pid`).
pub(crate) fn runs_profile(lock_pid: Option<u32>) -> impl Fn(&ProcessView) -> bool {
    move |process: &ProcessView| is_chrome_family(process) && Some(process.pid) == lock_pid
}

fn gone(record: &UserChromeRecord) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "Your {} at {} no longer has remote debugging on",
            record.name,
            record.user_data_dir.display()
        ),
    )
    .with_suggestion(AGAIN)
    .with_details(serde_json::json!({ "kind": "cdp_user_chrome_gone" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn not_the_profile(record: &UserChromeRecord, port: u16, verdict: &ListenerVerdict) -> AppError {
    let holder = match verdict {
        ListenerVerdict::Untrusted {
            holder: Some(holder),
        } => format!("held by {holder}"),
        ListenerVerdict::CantCheck(reason) => format!("could not be checked ({reason})"),
        _ => "held by a process of another user or of root".to_owned(),
    };
    AppError::new(
        ErrorCode::PermDenied,
        format!(
            "The profile at {} names port {port}, which is {holder}, not the {} running that \
             profile; tabref does not attach to it",
            record.user_data_dir.display(),
            record.name
        ),
    )
    .with_suggestion(AGAIN)
    .with_details(serde_json::json!({ "kind": "cdp_endpoint_unverified", "port": port }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// The user-data directory shown to the user: `~/…` under the home folder.
pub(crate) fn shown_dir(dir: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.and_then(|home| dir.strip_prefix(&home).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => dir.display().to_string(),
    }
}

#[cfg(test)]
#[path = "user_chrome_tests.rs"]
mod tests;
