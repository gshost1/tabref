//! Where the user's own browsers keep their profiles. Only names are built
//! here, from the environment; nothing on disk is read, so a caller can ask
//! whether a directory is the user's profile without touching it. Reading a
//! profile (its `DevToolsActivePort`) is for `tabref connect --user-chrome`
//! alone.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::user_browser::UserBrowser;

#[cfg(target_os = "macos")]
const DEFAULT_LOCATIONS: &[(&str, &str)] = &[
    ("Google Chrome", "Google/Chrome"),
    ("Google Chrome Canary", "Google/Chrome Canary"),
    ("Chromium", "Chromium"),
    ("Microsoft Edge", "Microsoft Edge"),
    ("Brave", "BraveSoftware/Brave-Browser"),
];

#[cfg(target_os = "linux")]
const DEFAULT_LOCATIONS: &[(&str, &str)] = &[
    ("Google Chrome", "google-chrome"),
    ("Google Chrome Beta", "google-chrome-beta"),
    ("Google Chrome Dev", "google-chrome-unstable"),
    ("Chromium", "chromium"),
    ("Microsoft Edge", "microsoft-edge"),
    ("Brave", "BraveSoftware/Brave-Browser"),
];

#[cfg(target_os = "windows")]
const DEFAULT_LOCATIONS: &[(&str, &str)] = &[
    ("Google Chrome", "Google/Chrome/User Data"),
    ("Google Chrome Canary", "Google/Chrome SxS/User Data"),
    ("Chromium", "Chromium/User Data"),
    ("Microsoft Edge", "Microsoft/Edge/User Data"),
    ("Brave", "BraveSoftware/Brave-Browser/User Data"),
];

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
const DEFAULT_LOCATIONS: &[(&str, &str)] = &[];

/// Every known browser's default user-data directory on this OS, whether or
/// not it exists.
pub fn default_user_data_dirs() -> Vec<UserBrowser> {
    user_data_dirs_with(|name| std::env::var_os(name))
}

/// Whether `dir` is one of the user's own browsers' default user-data
/// directories: the place their logins live, which only an explicit opt-in
/// (`tabref connect --user-chrome`) may hand to tabref.
pub fn is_default_user_data_dir(dir: &Path) -> bool {
    is_default_in(dir, &default_user_data_dirs())
}

/// Whether `dir` is one of `browsers`' user-data directories.
pub(crate) fn is_default_in(dir: &Path, browsers: &[UserBrowser]) -> bool {
    let wanted = comparable(dir);
    browsers
        .iter()
        .any(|browser| comparable(&browser.user_data_dir) == wanted)
}

/// A path as two spellings of the same directory compare equal: absolute,
/// with symlinks resolved where it exists.
fn comparable(path: &Path) -> PathBuf {
    std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn user_data_dirs_with(lookup: impl Fn(&str) -> Option<OsString>) -> Vec<UserBrowser> {
    let Some(base) = base_dir(&lookup) else {
        return Vec::new();
    };
    DEFAULT_LOCATIONS
        .iter()
        .map(|(name, relative)| UserBrowser {
            name,
            user_data_dir: relative
                .split('/')
                .fold(base.clone(), |path, part| path.join(part)),
        })
        .collect()
}

fn non_empty(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn base_dir(lookup: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    non_empty(lookup("HOME")).map(|home| home.join("Library").join("Application Support"))
}

#[cfg(target_os = "linux")]
fn base_dir(lookup: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    non_empty(lookup("XDG_CONFIG_HOME"))
        .or_else(|| non_empty(lookup("HOME")).map(|home| home.join(".config")))
}

#[cfg(target_os = "windows")]
fn base_dir(lookup: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    non_empty(lookup("LOCALAPPDATA"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn base_dir(lookup: &impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    non_empty(lookup("HOME"))
}

#[cfg(test)]
#[path = "default_profiles_tests.rs"]
mod tests;
