//! Which browser record a command would be served by, as a text a kept
//! daemon connection can be compared against: the user's own browser
//! (`connect --user-chrome`), the browser `open --launch` started, or a
//! `--profile` no recorded browser runs yet. A kept connection made under
//! another choice is not reused (see `kept_session`), so turning the opt-in
//! on or off, a new launch, or a launch on another profile always reaches
//! the browser it names.

use tabref_core::RefStore;

use super::launch_profile::{requested_profile, same_dir};
use super::launch_record::{load_launched, read_launched};
use super::session_options::SessionOptions;
use super::user_chrome_record::choice_in_use;

/// The choice a command with `options` is served under; `None` with
/// `--cdp` or when no record chooses (the implicit 9222).
pub(crate) fn choice_key(store: &RefStore, options: &SessionOptions) -> Option<String> {
    if options.cdp.is_some() {
        return None;
    }
    if let Some((text, _)) = choice_in_use(store, options) {
        return Some(format!("user:{text}"));
    }
    if let Some(profile) = requested_profile(options) {
        let recorded =
            load_launched(store).is_some_and(|record| same_dir(profile, &record.profile_dir));
        if !recorded {
            return Some(format!("profile:{}", profile.display()));
        }
    }
    read_launched(store).map(|text| format!("launched:{text}"))
}

#[cfg(test)]
#[path = "browser_choice_tests.rs"]
mod tests;
