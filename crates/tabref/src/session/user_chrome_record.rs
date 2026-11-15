//! The user's own browser, as `tabref connect --user-chrome` chose it. Its
//! presence in the state directory is the opt-in: without it, no command
//! reads any of the user's profile directories. `tabref connect --off`
//! removes it.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tabref_core::{AppError, RefStore};

pub(crate) const USER_CHROME_FILE: &str = "user_chrome.json";

/// The browser brand and the user-data directory the user chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct UserChromeRecord {
    pub(crate) name: String,
    pub(crate) user_data_dir: PathBuf,
}

pub(crate) fn load_user_chrome(store: &RefStore) -> Option<UserChromeRecord> {
    serde_json::from_str(&store.read_state(USER_CHROME_FILE)?).ok()
}

pub(crate) fn save_user_chrome(
    store: &RefStore,
    record: &UserChromeRecord,
) -> Result<(), AppError> {
    store.write_state(USER_CHROME_FILE, &serde_json::to_string(record)?)
}

pub(crate) fn forget_user_chrome(store: &RefStore) {
    store.remove_state(USER_CHROME_FILE);
}
