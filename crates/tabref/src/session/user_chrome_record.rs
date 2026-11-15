//! The user's own browser, as `tabref connect --user-chrome` chose it. Its
//! presence in the state directory is the opt-in: without it, no command
//! reads any of the user's profile directories. It names the very browser
//! process the user opted in to (its pid and start time), so a browser that
//! exits or restarts needs a new opt-in. `tabref connect --off` removes it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tabref_core::{AppError, RefStore};

use super::session_options::SessionOptions;

pub(crate) const USER_CHROME_FILE: &str = "user_chrome.json";

/// The browser brand, its user-data directory, and the browser process that
/// held that profile at the opt-in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UserChromeRecord {
    pub(crate) name: String,
    pub(crate) user_data_dir: PathBuf,
    pub(crate) pid: u32,
    pub(crate) started: Option<String>,
}

/// The record in `text`, the state file's contents.
pub(crate) fn parse_user_chrome(text: &str) -> Option<UserChromeRecord> {
    serde_json::from_str(text).ok()
}

/// The user's browser a command with `options` would use, read once, with
/// the state file's text it came from: none with `--cdp` (the user named an
/// endpoint) or with `open --launch` (tabref's own browser is asked for).
pub(crate) fn choice_in_use(
    store: &RefStore,
    options: &SessionOptions,
) -> Option<(String, UserChromeRecord)> {
    if options.cdp.is_some() || options.launch.is_some() {
        return None;
    }
    let text = store.read_state(USER_CHROME_FILE)?;
    let record = parse_user_chrome(&text)?;
    Some((text, record))
}

pub(crate) fn load_user_chrome(store: &RefStore) -> Option<UserChromeRecord> {
    parse_user_chrome(&store.read_state(USER_CHROME_FILE)?)
}

pub(crate) fn save_user_chrome(
    store: &RefStore,
    record: &UserChromeRecord,
) -> Result<(), AppError> {
    store.write_state(USER_CHROME_FILE, &serde_json::to_string(record)?)
}

/// Removes the record and confirms it is gone: a record left behind would
/// keep later commands in the user's browser.
pub(crate) fn forget_user_chrome(store: &RefStore) -> Result<(), AppError> {
    store.remove_state(USER_CHROME_FILE);
    if store.read_state(USER_CHROME_FILE).is_some() {
        return Err(AppError::internal(format!(
            "{} could not be removed from {}; delete it to stop using your browser",
            USER_CHROME_FILE,
            store.base_dir().display()
        )));
    }
    Ok(())
}
