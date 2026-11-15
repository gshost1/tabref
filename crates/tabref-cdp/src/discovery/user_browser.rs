use std::path::PathBuf;

/// A browser of the user's own, as tabref names it: its brand and the
/// user-data directory that holds its profiles and logins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserBrowser {
    pub name: &'static str,
    pub user_data_dir: PathBuf,
}
