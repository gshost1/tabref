//! Files in a download folder the browser saves to under each download's
//! guid: whether one has landed, and giving it the name the server
//! suggested without ever replacing a file already there.

use std::path::{Path, PathBuf};

const PARTIAL_SUFFIX: &str = ".crdownload";
const FALLBACK_NAME: &str = "download";
const MAX_COPIES: u32 = 1000;

/// The size of the finished guid-named file in `dir`, or `None` while it
/// is missing or still being written (`<guid>.crdownload`). A symbolic link
/// is never taken for a download.
pub(crate) fn landed_size(dir: &Path, guid: &str) -> Option<u64> {
    let name = plain_name(guid)?;
    if regular_len(&dir.join(format!("{name}{PARTIAL_SUFFIX}"))).is_some() {
        return None;
    }
    regular_len(&dir.join(name))
}

/// Moves the finished guid-named file to the suggested name in the same
/// folder, as `name (1).ext` when that name is taken. Each candidate is
/// claimed with a hard link, which fails when the name exists, and the guid
/// name is removed only after the link holds; a folder without hard links
/// falls back to a rename once the name is seen to be free.
pub(crate) fn place(dir: &Path, guid: &str, suggested: &str) -> std::io::Result<PathBuf> {
    let source = plain_name(guid)
        .map(|name| dir.join(name))
        .filter(|path| regular_len(path).is_some())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "the download is not a regular file in the download folder",
            )
        })?;
    let name = safe_file_name(suggested);
    for copy in 0..MAX_COPIES {
        let target = dir.join(numbered(&name, copy));
        match std::fs::hard_link(&source, &target) {
            Ok(()) => {
                std::fs::remove_file(&source)?;
                return Ok(target);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) if !target.exists() => {
                std::fs::rename(&source, &target)?;
                return Ok(target);
            }
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "every numbered copy of the name is taken",
    ))
}

/// A file name that stays inside the folder: the last path component,
/// control and reserved characters replaced, never empty or only dots.
pub(crate) fn safe_file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let cleaned: String = last
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']).trim_start();
    if trimmed.is_empty() || trimmed.chars().all(|ch| ch == '.') {
        return FALLBACK_NAME.to_owned();
    }
    trimmed.to_owned()
}

/// `name` for the first copy, then `stem (n).ext`.
pub(crate) fn numbered(name: &str, copy: u32) -> String {
    if copy == 0 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} ({copy}).{extension}"),
        _ => format!("{name} ({copy})"),
    }
}

fn plain_name(guid: &str) -> Option<&str> {
    let plain = !guid.is_empty()
        && guid
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-');
    plain.then_some(guid)
}

fn regular_len(path: &Path) -> Option<u64> {
    std::fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.file_type().is_file())
        .map(|metadata| metadata.len())
}

#[cfg(test)]
#[path = "download_place_tests.rs"]
mod tests;
