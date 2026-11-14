use std::path::{Path, PathBuf};

use crate::AppError;

/// The environment variable that moves tabref's state directory.
pub const STATE_ROOT_ENV: &str = "TABREF_HOME";
const STATE_DIR_NAME: &str = ".tabref";

/// The directory tabref keeps snapshots, the current tab and the browser it
/// launched in: `$TABREF_HOME` when set, otherwise `~/.tabref`.
pub fn resolve_state_root() -> Result<PathBuf, AppError> {
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from);
    root_from(std::env::var_os(STATE_ROOT_ENV).map(PathBuf::from), home)
}

pub(crate) fn root_from(
    configured: Option<PathBuf>,
    home: Option<PathBuf>,
) -> Result<PathBuf, AppError> {
    if let Some(path) = configured {
        validate_configured(&path)?;
        return Ok(path);
    }
    home.map(|home| home.join(STATE_DIR_NAME)).ok_or_else(|| {
        AppError::internal("No home directory to keep state in")
            .with_suggestion(format!("Set {STATE_ROOT_ENV} to an absolute directory"))
    })
}

fn validate_configured(path: &Path) -> Result<(), AppError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(AppError::invalid_args(format!(
            "{STATE_ROOT_ENV} must be an absolute path, got '{}'",
            path.display()
        )));
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(AppError::invalid_args(format!(
            "{STATE_ROOT_ENV} must not be a symlink: '{}'",
            path.display()
        ))),
        Ok(meta) if !meta.is_dir() => Err(AppError::invalid_args(format!(
            "{STATE_ROOT_ENV} must be a directory: '{}'",
            path.display()
        ))),
        _ => Ok(()),
    }
}

#[cfg(test)]
#[path = "state_root_tests.rs"]
mod tests;
