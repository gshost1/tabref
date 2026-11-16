use std::fs::File;

use tabref_cdp::LaunchedBrowser;
use tabref_core::{AppError, RefStore};

pub(crate) const LAUNCH_FILE: &str = "launched.json";
const LAUNCH_LOCK: &str = "launched.lock";

/// The browser `open --launch` started, as the state directory remembers it.
pub(crate) fn load_launched(store: &RefStore) -> Option<LaunchedBrowser> {
    parse_launched(&read_launched(store)?)
}

/// The launch record's text, when the file is this user's and no one else
/// may write it (Unix); a record anyone else could have written is ignored,
/// as if there were none.
pub(crate) fn read_launched(store: &RefStore) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::symlink_metadata(store.base_dir().join(LAUNCH_FILE)).ok()?;
        let own = meta.uid() == rustix::process::geteuid().as_raw();
        if !meta.is_file() || !own || meta.mode() & 0o022 != 0 {
            return None;
        }
    }
    store.read_state(LAUNCH_FILE)
}

/// The record in `text`, the state file's contents.
pub(crate) fn parse_launched(text: &str) -> Option<LaunchedBrowser> {
    serde_json::from_str(text).ok()
}

/// Saves the record and returns the text it saved, while no
/// `close --browser` is working on the record (see `lock_launched`).
pub(crate) fn save_launched(
    store: &RefStore,
    browser: &LaunchedBrowser,
) -> Result<String, AppError> {
    let _turn = lock_launched(store)?;
    let text = serde_json::to_string(browser)?;
    store.write_state(LAUNCH_FILE, &text)?;
    Ok(text)
}

pub(crate) fn forget_launched(store: &RefStore) {
    store.remove_state(LAUNCH_FILE);
}

/// Takes the state directory's launch-record lock (a `flock` on a private
/// file beside the record), held for as long as the returned file is open:
/// `close --browser` holds it while it closes the recorded browser, so a
/// launch cannot save a new record in between.
pub(crate) fn lock_launched(store: &RefStore) -> Result<File, AppError> {
    tabref_core::create_private_dir(store.base_dir())?;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(store.base_dir().join(LAUNCH_LOCK))?;
    file.lock()?;
    Ok(file)
}

#[cfg(all(test, unix))]
#[path = "launch_record_tests.rs"]
mod tests;
