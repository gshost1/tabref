use std::io::Write;
use std::path::Path;

use serde_json::{Value, json};

pub(super) fn record(event: &str, details: impl FnOnce() -> Value) {
    let Some(path) = std::env::var_os("TABREF_CAPTURE_TRACE") else {
        return;
    };
    record_to(Path::new(&path), event, details());
}

fn record_to(path: &Path, event: &str, details: Value) {
    let Some(mut file) = private_file(path) else {
        return;
    };
    let entry = json!({"event":event, "details":redact(details)});
    let _ = writeln!(file, "{entry}");
}

fn private_file(path: &Path) -> Option<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(unix_flags()?);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if regular(&metadata) => {
            let file = options.open(path).ok()?;
            let opened = file.metadata().ok()?;
            if !regular(&opened) {
                return None;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                if opened.dev() != metadata.dev() || opened.ino() != metadata.ino() {
                    return None;
                }
                file.set_permissions(std::fs::Permissions::from_mode(0o600))
                    .ok()?;
            }
            Some(file)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            options.create_new(true).open(path).ok()
        }
        _ => None,
    }
}

#[cfg(unix)]
fn unix_flags() -> Option<i32> {
    if cfg!(all(
        any(target_os = "linux", target_os = "android"),
        any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "arm",
            target_arch = "riscv64"
        )
    )) {
        const O_NOFOLLOW: i32 = 0x20000;
        const O_NONBLOCK: i32 = 0x800;
        Some(O_NOFOLLOW | O_NONBLOCK)
    } else if cfg!(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )) {
        const O_NOFOLLOW: i32 = 0x100;
        const O_NONBLOCK: i32 = 0x4;
        Some(O_NOFOLLOW | O_NONBLOCK)
    } else {
        None
    }
}

fn regular(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

fn redact_url_part(part: &str) -> String {
    let end = part.find(['?', '#']).unwrap_or(part.len());
    let mut clean = part[..end].to_owned();
    let mut offset = 0;
    while let Some(scheme) = clean[offset..].find("://") {
        let start = offset + scheme + 3;
        let end = clean[start..]
            .find(['/', '\'', '"', ')', ',', '<', '>'])
            .map_or(clean.len(), |n| start + n);
        if let Some(at) = clean[start..end].rfind('@') {
            clean.replace_range(start..start + at + 1, "");
        }
        offset = start;
    }
    if end < part.len() {
        clean.push_str(&part[part.trim_end().len()..]);
    }
    clean
}

fn redact(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(
            text.split_inclusive(char::is_whitespace)
                .map(redact_url_part)
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(redact).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .filter(|(key, _)| key != "params")
                .map(|(key, value)| (key, redact(value)))
                .collect(),
        ),
        value => value,
    }
}

#[cfg(test)]
#[path = "capture_watch_trace_tests.rs"]
mod tests;
