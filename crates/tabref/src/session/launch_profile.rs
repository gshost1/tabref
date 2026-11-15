//! `open --launch --profile DIR`: a browser of tabref's own on a profile
//! that persists between runs (its logins and cookies stay), instead of a
//! throwaway one. The user's own browsers' default profiles are refused (to
//! drive those, `connect --user-chrome` is the opt-in), and so is tabref's
//! state directory. While a browser already runs on the profile, it is
//! reused rather than a second one started.

use std::path::{Path, PathBuf};

use tabref_cdp::{
    LaunchedBrowser, ListenerVerdict, ProcessView, check_listener, is_default_user_data_dir,
    process_start, profile_lock_pid, read_active_port,
};
use tabref_core::{AppError, DeliverySemantics, ErrorCode, resolve_state_root};

use super::endpoint_choice::launched_from;
use super::session_options::SessionOptions;

/// The persistent profile the command asks to launch on, if any.
pub(crate) fn requested_profile(options: &SessionOptions) -> Option<&Path> {
    options.launch.as_ref()?.profile_dir.as_deref()
}

/// Refuses a profile tabref must not launch on: one of the user's own
/// browsers' default profiles, or tabref's state directory (or anything in
/// or around it).
pub(crate) fn check_profile(dir: &Path) -> Result<(), AppError> {
    let state_root = resolve_state_root()?;
    refusal(dir, &state_root, &is_default_user_data_dir).map_or(Ok(()), Err)
}

pub(crate) fn refusal(
    dir: &Path,
    state_root: &Path,
    is_default: &dyn Fn(&Path) -> bool,
) -> Option<AppError> {
    if is_default(dir) {
        return Some(refused(
            dir,
            "is your own browser's profile, with your logins",
            "To drive that browser, opt in with 'tabref connect --user-chrome'; for a profile of \
             tabref's own, pass another --profile directory",
        ));
    }
    let (dir, root) = (comparable(dir), comparable(state_root));
    if dir.starts_with(&root) || root.starts_with(&dir) {
        return Some(refused(
            &dir,
            "is tabref's state directory, or holds it, or lies in it",
            "Pass a --profile directory outside tabref's state directory",
        ));
    }
    None
}

/// The browser already running on `dir` with remote debugging on, as a
/// launch record: its profile lock names the process, and that process,
/// running this profile, alone holds the port its `DevToolsActivePort`
/// gives. `None` when no such browser runs (or it cannot be checked).
pub(crate) fn running_on_profile(dir: &Path, headless: bool) -> Option<LaunchedBrowser> {
    let pid = profile_lock_pid(dir)?;
    let started = process_start(pid)?;
    let active = read_active_port(dir).ok()?;
    let ours = launched_from(dir);
    let runs = move |process: &ProcessView| process.pid == pid && ours(process);
    matches!(
        check_listener(active.port, &runs),
        ListenerVerdict::Trusted(_)
    )
    .then(|| LaunchedBrowser {
        pid,
        port: active.port,
        websocket_path: active.websocket_path,
        profile_dir: dir.to_path_buf(),
        headless,
        started: Some(started),
        persistent: true,
    })
}

/// Whether two spellings name the same directory.
pub(crate) fn same_dir(a: &Path, b: &Path) -> bool {
    comparable(a) == comparable(b)
}

fn comparable(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn refused(dir: &Path, what: &str, suggestion: &str) -> AppError {
    AppError::new(
        ErrorCode::InvalidArgs,
        format!(
            "--profile {} {what}; tabref does not launch on it",
            dir.display()
        ),
    )
    .with_suggestion(suggestion)
    .with_details(serde_json::json!({ "kind": "launch_profile_refused" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "launch_profile_tests.rs"]
mod tests;
