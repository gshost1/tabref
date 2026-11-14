use tabref_cdp::LaunchedBrowser;
use tabref_core::{AppError, RefStore};

const LAUNCH_FILE: &str = "launched.json";

/// The browser `open --launch` started, as the state directory remembers it.
pub(crate) fn load_launched(store: &RefStore) -> Option<LaunchedBrowser> {
    serde_json::from_str(&store.read_state(LAUNCH_FILE)?).ok()
}

pub(crate) fn save_launched(store: &RefStore, browser: &LaunchedBrowser) -> Result<(), AppError> {
    store.write_state(LAUNCH_FILE, &serde_json::to_string(browser)?)
}

pub(crate) fn forget_launched(store: &RefStore) {
    store.remove_state(LAUNCH_FILE);
}
