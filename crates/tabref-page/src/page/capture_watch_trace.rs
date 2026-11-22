use std::io::Write;

use serde_json::{Value, json};

pub(super) fn record(event: &str, details: impl FnOnce() -> Value) {
    let Some(path) = std::env::var_os("TABREF_CAPTURE_TRACE") else {
        return;
    };
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    let entry = json!({"event":event, "details":details()});
    let _ = writeln!(file, "{entry}");
}
