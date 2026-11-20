//! Attaching to the user's own, logged-in browser: opt-in only, through
//! `tabref connect --user-chrome`. The browser's `DevToolsActivePort` (written
//! when its user turns remote debugging on at chrome://inspect) gives the
//! port, and the profile's lock names the browser process. The opt-in holds
//! for that one process (pid and start time): a browser that exits or
//! restarts is gone, and needs a new opt-in. Before anything is sent to the
//! port, it must be held by that process and by no other (Linux: the socket
//! tables; macOS: `lsof`). Where that cannot be checked (Windows), tabref
//! does not attach to the user's browser at all. Every command run in it
//! says so in a warning.

use std::path::{Path, PathBuf};

use tabref_cdp::{
    ListenerVerdict, ProcessView, UserBrowser, check_listener, connect_held, connect_with,
    default_user_data_dirs, endpoint_from_user_data_dir, is_chrome_family, listener_pids,
    process_start, profile_lock_pid, read_active_port,
};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::connected_browser::ConnectedBrowser;
use super::user_chrome_record::UserChromeRecord;

const AGAIN: &str = "Turn remote debugging on at chrome://inspect/#remote-debugging and run \
                     'tabref connect --user-chrome' again";
pub(crate) const GONE_KIND: &str = "cdp_user_chrome_gone";
/// Whether this platform can say when a process started; where it can, an
/// opt-in or record without a start time is refused.
const START_TIMES: bool = cfg!(any(target_os = "linux", target_os = "macos"));

/// The user's browser to attach to: the one in `dir`, or the one default
/// profile whose browser runs with remote debugging on. A
/// `DevToolsActivePort` an exited browser left behind does not count while
/// another browser runs, even when both files name the same port. Only
/// these directories' `DevToolsActivePort` files and profile locks are
/// read.
pub(crate) fn find_user_chrome(dir: Option<&Path>) -> Result<UserChromeRecord, AppError> {
    if cfg!(windows) {
        return Err(unsupported());
    }
    let (name, dir) = match dir {
        Some(dir) => named_profile(dir)?,
        None => only_profile(default_user_data_dirs())?,
    };
    let pid = profile_lock_pid(&dir).ok_or_else(|| not_running(&dir))?;
    let started = process_start(pid);
    if START_TIMES && started.is_none() {
        return Err(not_running(&dir));
    }
    Ok(UserChromeRecord {
        name: name.to_owned(),
        user_data_dir: dir,
        pid,
        started,
    })
}

fn named_profile(dir: &Path) -> Result<(&'static str, PathBuf), AppError> {
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    read_active_port(&dir)?;
    let name = default_user_data_dirs()
        .into_iter()
        .find(|browser| browser.user_data_dir == dir)
        .map_or("Chrome", |browser| browser.name);
    Ok((name, dir))
}

/// The one browser of `browsers` that runs with remote debugging on. When
/// none runs, the first profile with a left-over `DevToolsActivePort` is
/// named as not running, so the error says which browser to start.
pub(crate) fn only_profile(
    browsers: Vec<UserBrowser>,
) -> Result<(&'static str, PathBuf), AppError> {
    let debugged: Vec<(&'static str, PathBuf)> = browsers
        .into_iter()
        .filter(|browser| read_active_port(&browser.user_data_dir).is_ok())
        .map(|browser| (browser.name, browser.user_data_dir))
        .collect();
    let running: Vec<&(&'static str, PathBuf)> =
        debugged.iter().filter(|(_, dir)| is_running(dir)).collect();
    match running.as_slice() {
        [one] => Ok((*one).clone()),
        [] => match debugged.first() {
            Some((_, left_over)) => Err(not_running(left_over)),
            None => Err(AppError::new(
                ErrorCode::ActionFailed,
                "None of your browsers has remote debugging turned on",
            )
            .with_suggestion(
                "Turn it on at chrome://inspect/#remote-debugging in your browser (it asks you to \
                 allow each connection), then run 'tabref connect --user-chrome' again; for a \
                 browser tabref does not know, add --user-data-dir DIR",
            )
            .with_details(serde_json::json!({ "kind": "cdp_user_chrome_not_found" }))
            .with_disposition(DeliverySemantics::not_delivered())),
        },
        several => Err(AppError::new(
            ErrorCode::AmbiguousTarget,
            format!(
                "{} of your running browsers have remote debugging on",
                several.len()
            ),
        )
        .with_suggestion(format!(
            "Name one with --user-data-dir: {}",
            several
                .iter()
                .map(|(_, dir)| dir.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
        .with_details(serde_json::json!({ "kind": "cdp_user_chrome_ambiguous" }))
        .with_disposition(DeliverySemantics::not_delivered())),
    }
}

/// Whether a live browser process holds the profile's lock.
fn is_running(dir: &Path) -> bool {
    match profile_lock_pid(dir) {
        Some(pid) => !START_TIMES || process_start(pid).is_some(),
        None => false,
    }
}

/// Connects to the user's browser `record` names: only while the very
/// process the user opted in to still holds the profile, and only after
/// checking that it alone holds the port its profile advertises. A browser
/// that exited or restarted is kind `cdp_user_chrome_gone`.
pub(crate) fn connect_user_chrome(
    record: &UserChromeRecord,
    deadline: Deadline,
) -> Result<ConnectedBrowser, AppError> {
    if !same_process(record) {
        return Err(gone(record));
    }
    let endpoint = endpoint_from_user_data_dir(&record.user_data_dir).map_err(|_| gone(record))?;
    let port = endpoint.port();
    let connection = match check_listener(port, &runs_profile(Some(record.pid))) {
        ListenerVerdict::Trusted(held) => connect_held(&endpoint, &held, deadline)?,
        ListenerVerdict::Unknown => {
            listener_is(port, record)?;
            connect_with(&endpoint, deadline, &|| listener_is(port, record))?
        }
        ListenerVerdict::Nothing => return Err(gone(record)),
        refused => return Err(not_the_profile(record, port, &refused)),
    };
    Ok(ConnectedBrowser {
        endpoint,
        connection,
        warning: Some(format!("your logged-in {}", record.name)),
        choice: None,
        started_headless_profile: false,
    })
}

/// Whether the profile is still held by the process the user opted in to:
/// same pid in the lock, and, where the platform says, same start time.
pub(crate) fn same_process(record: &UserChromeRecord) -> bool {
    if profile_lock_pid(&record.user_data_dir) != Some(record.pid) {
        return false;
    }
    match &record.started {
        Some(started) => process_start(record.pid).as_ref() == Some(started),
        None => !START_TIMES,
    }
}

/// Where the socket tables cannot be read (macOS), the port's listeners as
/// `lsof` reports them must all be the opted-in browser; with no such check
/// (Windows), tabref refuses.
fn listener_is(port: u16, record: &UserChromeRecord) -> Result<(), AppError> {
    listeners_are(port, record, listener_pids(port))
}

/// [`listener_is`] for the listener `pids` found (`None`: none could be
/// found on this platform).
pub(crate) fn listeners_are(
    port: u16,
    record: &UserChromeRecord,
    pids: Option<Vec<u32>>,
) -> Result<(), AppError> {
    match pids {
        Some(pids) if pids.is_empty() => Err(gone(record)),
        Some(pids) => match pids.iter().find(|pid| **pid != record.pid) {
            None => Ok(()),
            Some(stranger) => Err(not_the_profile(
                record,
                port,
                &ListenerVerdict::Untrusted {
                    holder: Some(format!("pid {stranger}")),
                },
            )),
        },
        None => Err(unsupported()),
    }
}

/// Whether a process is the browser holding the profile lock (`lock_pid`).
pub(crate) fn runs_profile(lock_pid: Option<u32>) -> impl Fn(&ProcessView) -> bool {
    move |process: &ProcessView| is_chrome_family(process) && Some(process.pid) == lock_pid
}

fn gone(record: &UserChromeRecord) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "Your {} at {} has closed, restarted or turned remote debugging off, so tabref no \
             longer uses it",
            record.name,
            shown_dir(&record.user_data_dir)
        ),
    )
    .with_suggestion(format!("{AGAIN} to use it again"))
    .with_details(serde_json::json!({ "kind": GONE_KIND }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn not_running(dir: &Path) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "No running browser holds the profile at {}; its DevToolsActivePort is left from an earlier run",
            shown_dir(dir)
        ),
    )
    .with_suggestion(AGAIN)
    .with_details(serde_json::json!({ "kind": "cdp_user_chrome_not_found" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn unsupported() -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        "tabref cannot yet check who holds your browser's DevTools port on this platform, so it \
         does not attach to your own browser here",
    )
    .with_suggestion("Run 'tabref open <url> --launch' for a browser of tabref's own")
    .with_details(serde_json::json!({ "kind": "cdp_user_chrome_unsupported" }))
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
            shown_dir(&record.user_data_dir),
            record.name
        ),
    )
    .with_suggestion(format!(
        "{AGAIN}, or 'tabref connect --off' to stop using it"
    ))
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
