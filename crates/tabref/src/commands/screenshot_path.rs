//! Where `tabref screenshot` writes its PNG: `--out FILE`, or a fresh
//! `tabref-screenshot-<id>.png` in the directory the command was typed in.
//! The file's folder must exist; it is resolved through links and `..`, and
//! a path in or under tabref's state directory is refused, so a screenshot
//! never lands among the files tabref trusts (the launch record, refs).

use std::path::{Path, PathBuf};

use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode, new_snapshot_id};

/// A new screenshot file name in `cwd`.
pub(crate) fn default_out(cwd: &Path) -> PathBuf {
    cwd.join(format!("tabref-screenshot-{}.png", new_snapshot_id()))
}

/// The file to write `out` to, with its folder resolved, unless it is in
/// or under `state_root`.
pub(crate) fn screenshot_path(out: &Path, state_root: &Path) -> Result<PathBuf, AppError> {
    let out = std::path::absolute(out)?;
    let (Some(folder), Some(name)) = (out.parent(), out.file_name()) else {
        return Err(argument(
            format!("--out {} names no file", out.display()),
            "screenshot_path_invalid",
        ));
    };
    let folder = std::fs::canonicalize(folder).map_err(|_| {
        argument(
            format!("The folder {} does not exist", folder.display()),
            "screenshot_folder_missing",
        )
    })?;
    let path = folder.join(name);
    if path.is_dir() {
        return Err(argument(
            format!("--out {} is a folder; name a file in it", path.display()),
            "screenshot_path_invalid",
        ));
    }
    let state = std::fs::canonicalize(state_root).unwrap_or_else(|_| state_root.to_path_buf());
    if path.starts_with(&state) {
        return Err(argument(
            format!(
                "{} is in tabref's state directory ({}), which keeps tabref's own files; a \
                 screenshot is never written there",
                path.display(),
                state.display()
            ),
            "screenshot_path_refused",
        )
        .with_suggestion("Pass --out with a file outside the state directory"));
    }
    Ok(path)
}

fn argument(message: String, kind: &str) -> AppError {
    AppError::new(ErrorCode::InvalidArgs, message)
        .with_details(json!({ "kind": kind }))
        .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "screenshot_path_tests.rs"]
mod tests;
