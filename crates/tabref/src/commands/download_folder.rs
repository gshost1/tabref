//! The download settings of each browser endpoint: the folder downloads
//! land in (the one `open --download-dir` chose, else a private folder in
//! tabref's state directory for a browser `open --launch` started, else
//! none, so the user's own browser keeps its own), and the size cap.

use std::path::{Component, Path, PathBuf};

use tabref_cdp::default_user_data_dirs;
use tabref_core::{AppError, create_private_dir};

use crate::session::Session;

const DEFAULT_CAP_MB: u64 = 500;
const MB: u64 = 1024 * 1024;

/// Saves `dir` (absolute, created private when new) as this browser's
/// download folder. tabref's state directory and the user's own browsers'
/// default profile directories, or a folder inside one, are refused before
/// anything is created, with symbolic links resolved: a page could
/// otherwise save a file named like tabref's state or a profile's there. A
/// dangling link or a parent-directory component is refused too.
pub(crate) fn choose_folder(session: &mut Session, dir: &Path) -> Result<PathBuf, AppError> {
    if dir.components().any(|part| part == Component::ParentDir) {
        return Err(AppError::invalid_args(
            "--download-dir cannot contain a '..' path component. Use the destination path directly.",
        ));
    }
    let absolute = std::path::absolute(dir)?;
    let root = session.store().base_dir().to_path_buf();
    if within(&absolute, &root)? {
        return Err(state_folder(&absolute));
    }
    if in_a_profile(&absolute)? {
        return Err(profile_folder(&absolute));
    }
    create_private_dir(&absolute)?;
    let name = state_name(session, "download-dir");
    session
        .store()
        .write_state(&name, &absolute.to_string_lossy())?;
    Ok(absolute)
}

/// Forgets the chosen folder: a browser tabref launched goes back to its
/// own folder in the state directory, any other to the folder it keeps.
pub(crate) fn clear_folder(session: &mut Session) {
    let name = state_name(session, "download-dir");
    session.store().remove_state(&name);
}

/// Saves the size cap, in megabytes, past which a download is canceled.
pub(crate) fn choose_cap(session: &mut Session, megabytes: u64) -> Result<(), AppError> {
    let ledger = super::download_ledger::DownloadLedger::of(session);
    if ledger.pending() {
        let folder = download_folder(session)?;
        let cap = download_cap(session);
        let deadline = session.deadline();
        ledger.with(|kept| {
            super::downloads::refresh_kept(
                session.connection(),
                kept,
                cap,
                folder.as_deref(),
                deadline,
            )
        })?;
    }
    let name = state_name(session, "download-cap-mb");
    session.store().write_state(&name, &megabytes.to_string())
}

/// The size cap in bytes: the chosen one, or 500 MB.
pub(crate) fn download_cap(session: &mut Session) -> u64 {
    let name = state_name(session, "download-cap-mb");
    session
        .store()
        .read_state(&name)
        .and_then(|text| text.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_CAP_MB)
        .saturating_mul(MB)
}

/// Whether `open --launch` started this browser, so tabref may keep its
/// downloads pointed at a folder between commands: the session's own
/// decision, made once when it opened, never the launch record's port
/// alone.
pub(crate) fn launched_here(session: &mut Session) -> bool {
    session.launched_by_tabref()
}

/// The folder this browser's downloads land in, or `None` when the
/// browser keeps its own. A chosen folder is checked again each time: one
/// that became unsafe (swapped for a link into the state directory or a
/// profile) is an error, never quietly replaced by the browser's own
/// folder.
pub(crate) fn download_folder(session: &mut Session) -> Result<Option<PathBuf>, AppError> {
    let name = state_name(session, "download-dir");
    let root = session.store().base_dir().to_path_buf();
    if let Some(chosen) = session.store().read_state(&name) {
        let chosen = PathBuf::from(chosen.trim());
        let safe = within(&chosen, &root).is_ok_and(|inside| !inside)
            && in_a_profile(&chosen).is_ok_and(|inside| !inside);
        if !safe {
            return Err(AppError::invalid_args(format!(
                "download folder {} is no longer safe; choose another with --download-dir",
                chosen.display()
            ))
            .with_details(serde_json::json!({ "kind": UNSAFE_FOLDER })));
        }
        return Ok(Some(chosen));
    }
    if !launched_here(session) {
        return Ok(None);
    }
    let folder = root.join("downloads");
    create_private_dir(&folder)?;
    let real = std::fs::symlink_metadata(&folder).is_ok_and(|meta| meta.is_dir());
    Ok(real.then_some(folder))
}

/// The `details.kind` of a chosen download folder that is no longer safe.
pub(crate) const UNSAFE_FOLDER: &str = "download_folder_unsafe";

/// Whether `dir` is `root` or inside it, as written and with symbolic
/// links resolved. Both paths may be missing, so resolve their nearest
/// existing ancestors. A dangling link in `dir` is `INVALID_ARGS`.
pub(crate) fn within(dir: &Path, root: &Path) -> Result<bool, AppError> {
    let root_absolute = std::path::absolute(root)?;
    if dir.starts_with(&root_absolute) {
        return Ok(true);
    }
    let real_root = resolved(&root_absolute).unwrap_or(root_absolute);
    Ok(resolved(dir)?.starts_with(real_root))
}

/// `dir` with its nearest existing ancestor's links resolved and the rest
/// appended as written.
fn resolved(dir: &Path) -> Result<PathBuf, AppError> {
    let mut existing = dir.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(meta) = std::fs::symlink_metadata(&existing) {
            if meta.file_type().is_symlink() && std::fs::metadata(&existing).is_err() {
                return Err(AppError::invalid_args(format!(
                    "{} is a link to a folder that does not exist; choose another download folder",
                    existing.display()
                )));
            }
            let mut real = existing.canonicalize()?;
            for part in rest.iter().rev() {
                real.push(part);
            }
            return Ok(real);
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => return Ok(dir.to_path_buf()),
        }
    }
}

/// Whether `dir` is one of the user's own browsers' default profile
/// directories or inside one.
pub(crate) fn in_a_profile(dir: &Path) -> Result<bool, AppError> {
    for browser in default_user_data_dirs() {
        if within(dir, &browser.user_data_dir)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn profile_folder(dir: &Path) -> AppError {
    AppError::invalid_args(format!(
        "{} is a browser's own profile directory or inside it; choose another download folder",
        dir.display()
    ))
}

fn state_folder(dir: &Path) -> AppError {
    AppError::invalid_args(format!(
        "{} is tabref's state directory or inside it; choose another download folder",
        dir.display()
    ))
}

/// The state file name of `kind` for this browser endpoint.
pub(crate) fn state_name(session: &Session, kind: &str) -> String {
    format!("{kind}-{}", session.endpoint().port())
}

#[cfg(test)]
#[path = "download_folder_tests.rs"]
mod tests;
