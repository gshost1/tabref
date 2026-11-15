//! `open --launch --profile DIR`: a browser of tabref's own on a profile
//! that persists between runs (its logins and cookies stay), instead of a
//! throwaway one. The path is normalised once (its deepest existing ancestor
//! resolved, a `..` past a missing directory refused) and that one spelling
//! is checked, created, given to Chrome and recorded. tabref launches only on
//! a profile it made: a new or empty directory, which it marks with
//! `.tabref-profile`, or one that has that mark. The user's own browsers'
//! default directories (and anything in or around them), tabref's state
//! directory, and a directory another user owns or others may write are
//! refused. A browser already running on the profile is reused rather than a
//! second one started; launches on one profile take turns through a lock.

use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use tabref_cdp::{
    LaunchedBrowser, ListenerVerdict, ProcessView, check_listener, default_user_data_dirs,
    is_chrome_family, process_start, profile_lock_pid, read_active_port,
};
use tabref_core::{
    AppError, Deadline, DeliverySemantics, ErrorCode, create_private_dir, resolve_state_root,
};

use super::endpoint_choice::launched_from;
use super::session_options::SessionOptions;

pub(crate) const PROFILE_MARKER: &str = ".tabref-profile";
const LAUNCH_LOCK: &str = ".tabref-launch.lock";
const LOCK_POLL: Duration = Duration::from_millis(25);

/// The persistent profile the command asks to launch on, if any, as given.
pub(crate) fn requested_profile(options: &SessionOptions) -> Option<&Path> {
    options.launch.as_ref()?.profile_dir.as_deref()
}

/// `dir` as one absolute spelling: its deepest existing ancestor with
/// symlinks and `..` resolved, the rest appended as written. A `..` past a
/// directory that does not exist cannot be resolved, so it is refused.
pub(crate) fn normalize(dir: &Path) -> Result<PathBuf, AppError> {
    let absolute = std::path::absolute(dir)?;
    let mut resolved = PathBuf::new();
    let mut missing = false;
    for component in absolute.components() {
        if !missing {
            match std::fs::canonicalize(resolved.join(component)) {
                Ok(next) => {
                    resolved = next;
                    continue;
                }
                Err(_) => missing = true,
            }
        }
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(refused(
                    dir,
                    "goes up ('..') past a directory that does not exist",
                    "Pass the profile directory without '..'",
                ));
            }
            other => resolved.push(other),
        }
    }
    Ok(resolved)
}

/// The profile to launch on, ready: normalised, checked, created (0700) and
/// marked when new or empty, in its one canonical spelling, with the lock
/// that makes launches on it take turns (held until the returned file is
/// dropped).
pub(crate) fn prepare_profile(dir: &Path, deadline: Deadline) -> Result<(PathBuf, File), AppError> {
    let dir = normalize(dir)?;
    let state_root = normalize(&resolve_state_root()?)?;
    let defaults: Vec<PathBuf> = default_user_data_dirs()
        .into_iter()
        .filter_map(|browser| normalize(&browser.user_data_dir).ok())
        .collect();
    if let Some(error) = refusal(&dir, &state_root, &defaults) {
        return Err(error);
    }
    if let Some(error) = unfit_existing(&dir) {
        return Err(error);
    }
    if !dir.exists() {
        create_private_dir(&dir)?;
    }
    mark(&dir)?;
    let dir = std::fs::canonicalize(&dir)?;
    let lock = lock(&dir, deadline)?;
    Ok((dir, lock))
}

/// Refuses a normalised profile in or around one of `defaults` (the user's
/// own browsers' profiles) or `state_root`.
pub(crate) fn refusal(dir: &Path, state_root: &Path, defaults: &[PathBuf]) -> Option<AppError> {
    let near = |other: &Path| dir.starts_with(other) || other.starts_with(dir);
    if defaults.iter().any(|default| near(default)) {
        return Some(refused(
            dir,
            "is your own browser's profile directory, or in or around it",
            "To drive your own browser, opt in with 'tabref connect --user-chrome'; for a profile \
             of tabref's own, pass a new directory",
        ));
    }
    if near(state_root) {
        return Some(refused(
            dir,
            "is tabref's state directory, or holds it, or lies in it",
            "Pass a --profile directory outside tabref's state directory",
        ));
    }
    None
}

/// Refuses an existing directory tabref did not make: not a directory, owned
/// by another user or writable by others (Unix), or holding files without
/// tabref's mark.
pub(crate) fn unfit_existing(dir: &Path) -> Option<AppError> {
    let metadata = std::fs::metadata(dir).ok()?;
    if !metadata.is_dir() {
        return Some(refused(dir, "is not a directory", "Pass a directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let own = metadata.uid() == rustix::process::geteuid().as_raw();
        if !own || metadata.permissions().mode() & 0o022 != 0 {
            return Some(refused(
                dir,
                "is owned by another user or writable by others",
                "Pass a directory of your own that only you can write",
            ));
        }
    }
    let empty = std::fs::read_dir(dir).map_or(true, |mut entries| entries.next().is_none());
    if !empty && !dir.join(PROFILE_MARKER).exists() {
        return Some(refused(
            dir,
            "is not a tabref profile (it has files and no .tabref-profile mark)",
            "For your own browser, use 'tabref connect --user-chrome'; for a profile of tabref's \
             own, pass a new or empty directory",
        ));
    }
    None
}

fn mark(dir: &Path) -> Result<(), AppError> {
    let marker = dir.join(PROFILE_MARKER);
    if marker.exists() {
        return Ok(());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(marker)?;
    std::io::Write::write_all(&mut file, b"a browser profile tabref made\n")?;
    Ok(())
}

fn lock(dir: &Path, deadline: Deadline) -> Result<File, AppError> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LAUNCH_LOCK))?;
    while file.try_lock().is_err() {
        if deadline.is_expired() {
            return Err(AppError::timeout(
                "Another launch on this profile did not finish before the deadline",
            ));
        }
        std::thread::sleep(LOCK_POLL);
    }
    Ok(file)
}

/// The browser already running on `dir` with remote debugging on, as a
/// launch record: its profile lock names the process, and that process, a
/// Chrome-family browser running this profile, alone holds the port its
/// `DevToolsActivePort` gives. `None` when no such browser runs (or it
/// cannot be checked).
pub(crate) fn running_on_profile(dir: &Path, headless: bool) -> Option<LaunchedBrowser> {
    let pid = profile_lock_pid(dir)?;
    let started = process_start(pid)?;
    let active = read_active_port(dir).ok()?;
    let ours = launched_from(dir);
    let runs = move |process: &ProcessView| {
        process.pid == pid && is_chrome_family(process) && ours(process)
    };
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
    match (normalize(a), normalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
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
