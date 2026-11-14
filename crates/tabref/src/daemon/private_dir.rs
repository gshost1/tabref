use std::fs::DirBuilder;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

use tabref_core::{AppError, ErrorCode};

use super::daemon_address::DaemonAddress;

/// Makes sure the daemon folder exists, is a real folder (not a link), is
/// owned by the user who owns the state root, and is closed to everyone
/// else. `/tmp` is shared, so a folder someone else made first is refused
/// rather than trusted.
pub(crate) fn ensure_private_dir(address: &DaemonAddress) -> Result<(), AppError> {
    std::fs::create_dir_all(&address.state_root)?;
    let owner = std::fs::metadata(&address.state_root)?.uid();
    match DirBuilder::new().mode(0o700).create(&address.dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    check_private(&address.dir, owner)
}

/// Whether `dir` is a private folder of `owner`, without creating it.
pub(crate) fn check_private(dir: &Path, owner: u32) -> Result<(), AppError> {
    let metadata = std::fs::symlink_metadata(dir)?;
    let private = metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == owner
        && metadata.permissions().mode() & 0o077 == 0;
    if private {
        return Ok(());
    }
    Err(AppError::new(
        ErrorCode::PermDenied,
        format!("{} is not a private folder of this user", dir.display()),
    )
    .with_suggestion("Remove it, or set TABREF_NO_DAEMON=1 to run without the daemon")
    .with_details(serde_json::json!({ "kind": "daemon_dir_unsafe" })))
}
