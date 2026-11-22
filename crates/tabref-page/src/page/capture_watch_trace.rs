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
        options.mode(0o600);
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            let file = options.open(path).ok()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                let opened = file.metadata().ok()?;
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

fn redact(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(
            text.split_inclusive(char::is_whitespace)
                .map(|part| {
                    let end = part.find(['?', '#']).unwrap_or(part.len());
                    if end == part.len() {
                        part.to_owned()
                    } else {
                        format!("{}{}", &part[..end], &part[part.trim_end().len()..])
                    }
                })
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
