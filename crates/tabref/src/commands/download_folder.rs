//! The folder downloads land in, per browser endpoint: the one `open
//! --download-dir` chose, else a folder in tabref's state directory for a
//! browser `open --launch` started, else none, so the downloads of the
//! user's own browser stay where that browser keeps them. The downloads not
//! yet returned by `wait --download` are kept beside it.

use std::path::{Path, PathBuf};

use tabref_core::AppError;
use tabref_page::DownloadRecord;

use crate::session::Session;
use crate::session::launch_record::load_launched;

const LEDGER_KEPT: usize = 50;

/// Saves `dir`, made absolute and created, as the download folder of this
/// browser.
pub(crate) fn choose_folder(session: &mut Session, dir: &Path) -> Result<PathBuf, AppError> {
    let absolute = std::path::absolute(dir)?;
    std::fs::create_dir_all(&absolute)?;
    let name = state_name(session, "download-dir");
    session
        .store()
        .write_state(&name, &absolute.to_string_lossy())?;
    Ok(absolute)
}

/// The folder this browser's downloads land in, created, or `None` when
/// the browser keeps its own.
pub(crate) fn download_folder(session: &mut Session) -> Option<PathBuf> {
    let name = state_name(session, "download-dir");
    if let Some(chosen) = session.store().read_state(&name) {
        return Some(PathBuf::from(chosen.trim()));
    }
    let port = session.endpoint().port();
    let launched = load_launched(session.store()).is_some_and(|browser| browser.port == port);
    if !launched {
        return None;
    }
    let folder = session.store().base_dir().join("downloads");
    std::fs::create_dir_all(&folder).ok()?;
    Some(folder)
}

/// The downloads `wait --download` has not returned yet, oldest first.
pub(crate) fn load_ledger(session: &mut Session) -> Vec<DownloadRecord> {
    let name = state_name(session, "downloads");
    session
        .store()
        .read_state(&name)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keeps `records` (the newest 50) for a later `wait --download`.
pub(crate) fn save_ledger(
    session: &mut Session,
    records: &[DownloadRecord],
) -> Result<(), AppError> {
    let name = state_name(session, "downloads");
    let kept = &records[records.len().saturating_sub(LEDGER_KEPT)..];
    session
        .store()
        .write_state(&name, &serde_json::to_string(kept)?)
}

fn state_name(session: &Session, kind: &str) -> String {
    format!("{kind}-{}", session.endpoint().port())
}
