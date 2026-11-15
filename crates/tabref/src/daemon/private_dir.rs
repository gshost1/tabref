use std::fs::DirBuilder;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::path::Path;

use tabref_core::{AppError, ErrorCode};

use super::daemon_address::DaemonAddress;

/// The effective user id of this process: the only user a daemon folder,
/// socket or peer may belong to.
pub(crate) fn effective_uid() -> u32 {
    rustix::process::geteuid().as_raw()
}

/// Makes sure the daemon folder exists, is a real folder (not a link), is
/// owned by this process's effective user, and is closed to everyone else.
/// `/tmp` is shared, so a folder someone else made first is refused rather
/// than trusted.
pub(crate) fn ensure_private_dir(address: &DaemonAddress) -> Result<(), AppError> {
    match DirBuilder::new().mode(0o700).create(&address.dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    check_private(&address.dir)
}

/// Whether `dir` is a private folder of this process's effective user,
/// without creating it.
pub(crate) fn check_private(dir: &Path) -> Result<(), AppError> {
    let metadata = std::fs::symlink_metadata(dir)?;
    let private = metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.uid() == effective_uid()
        && metadata.permissions().mode() & 0o077 == 0;
    if private {
        return Ok(());
    }
    Err(unsafe_path(dir))
}

/// Whether `socket` is a socket this process's effective user owns and no
/// one else may open.
pub(crate) fn check_socket(socket: &Path) -> Result<(), AppError> {
    let metadata = std::fs::symlink_metadata(socket)?;
    let owned = metadata.file_type().is_socket()
        && metadata.uid() == effective_uid()
        && metadata.permissions().mode() & 0o077 == 0;
    if owned {
        return Ok(());
    }
    Err(unsafe_path(socket))
}

fn unsafe_path(path: &Path) -> AppError {
    AppError::new(
        ErrorCode::PermDenied,
        format!("{} is not private to this user", path.display()),
    )
    .with_suggestion("Remove it, or set TABREF_NO_DAEMON=1 to run without the daemon")
    .with_details(serde_json::json!({ "kind": "daemon_path_unsafe" }))
}
