use tabref_cdp::LaunchedBrowser;
use tabref_core::{AppError, RefStore};

pub(crate) const LAUNCH_FILE: &str = "launched.json";

/// The browser `open --launch` started, as the state directory remembers it.
pub(crate) fn load_launched(store: &RefStore) -> Option<LaunchedBrowser> {
    parse_launched(&store.read_state(LAUNCH_FILE)?)
}

/// The record in `text`, the state file's contents.
pub(crate) fn parse_launched(text: &str) -> Option<LaunchedBrowser> {
    serde_json::from_str(text).ok()
}

/// Saves the record and returns the text it saved.
pub(crate) fn save_launched(
    store: &RefStore,
    browser: &LaunchedBrowser,
) -> Result<String, AppError> {
    let text = serde_json::to_string(browser)?;
    store.write_state(LAUNCH_FILE, &text)?;
    Ok(text)
}

pub(crate) fn forget_launched(store: &RefStore) {
    store.remove_state(LAUNCH_FILE);
}
