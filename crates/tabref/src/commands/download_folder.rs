//! The download settings of each browser endpoint: the folder downloads
//! land in (the one `open --download-dir` chose, else a private folder in
//! tabref's state directory for a browser `open --launch` started, else
//! none, so the user's own browser keeps its own), and the size cap.

use std::path::{Path, PathBuf};

use tabref_core::{AppError, create_private_dir};

use crate::session::Session;
use crate::session::launch_record::load_launched;

const DEFAULT_CAP_MB: u64 = 500;
const MB: u64 = 1024 * 1024;

/// Saves `dir` (absolute, created private when new) as this browser's
/// download folder. tabref's state directory, or a folder inside it, is
/// refused, through symbolic links too: a page could otherwise save a file
/// named like tabref's own state there.
pub(crate) fn choose_folder(session: &mut Session, dir: &Path) -> Result<PathBuf, AppError> {
    let absolute = std::path::absolute(dir)?;
    let root = session.store().base_dir().to_path_buf();
    if within(&absolute, &root) {
        return Err(state_folder(&absolute));
    }
    create_private_dir(&absolute)?;
    if within(&absolute, &root) {
        return Err(state_folder(&absolute));
    }
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
/// downloads pointed at a folder between commands.
pub(crate) fn launched_here(session: &mut Session) -> bool {
    let port = session.endpoint().port();
    load_launched(session.store()).is_some_and(|browser| browser.port == port)
}

/// Whether a folder was chosen with `open --download-dir`.
pub(crate) fn folder_chosen(session: &mut Session) -> bool {
    let name = state_name(session, "download-dir");
    session.store().read_state(&name).is_some()
}

/// The folder this browser's downloads land in, or `None` when the
/// browser keeps its own.
pub(crate) fn download_folder(session: &mut Session) -> Option<PathBuf> {
    let name = state_name(session, "download-dir");
    if let Some(chosen) = session.store().read_state(&name) {
        return Some(PathBuf::from(chosen.trim()));
    }
    if !launched_here(session) {
        return None;
    }
    let folder = session.store().base_dir().join("downloads");
    create_private_dir(&folder).ok()?;
    Some(folder)
}

/// Whether `dir` is `root` or inside it, as written and, once both exist,
/// with their symbolic links resolved.
pub(crate) fn within(dir: &Path, root: &Path) -> bool {
    let root_absolute = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    if dir.starts_with(&root_absolute) {
        return true;
    }
    match (dir.canonicalize(), root.canonicalize()) {
        (Ok(dir), Ok(root)) => dir.starts_with(root),
        _ => false,
    }
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
