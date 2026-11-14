use std::io::Write;

use tabref_core::{AppError, ErrorPayload, Response};

use super::outcome::Outcome;

/// Prints a successful command: its text, or the JSON envelope.
pub(crate) fn emit_ok(command: &str, outcome: Outcome, json: bool) {
    let mut stdout = std::io::stdout().lock();
    if json {
        let envelope = Response::ok(command, outcome.data);
        let _ = writeln!(stdout, "{}", to_json(&envelope));
    } else if !outcome.text.is_empty() {
        let _ = writeln!(stdout, "{}", outcome.text);
    }
}

/// Prints a failed command: the JSON envelope on stdout, or the error as
/// text on stderr.
pub(crate) fn emit_error(command: &str, error: &AppError, json: bool) {
    if json {
        let envelope = Response::err(command, ErrorPayload::from_error(error));
        let _ = writeln!(std::io::stdout().lock(), "{}", to_json(&envelope));
        return;
    }
    let _ = writeln!(std::io::stderr().lock(), "{}", error_text(error));
}

pub(crate) fn error_text(error: &AppError) -> String {
    let mut text = format!("error {}", error.code.as_str());
    if let Some(kind) = error.kind() {
        text.push_str(&format!(" ({kind})"));
    }
    text.push_str(": ");
    text.push_str(&error.message);
    if let Some(detail) = &error.platform_detail {
        text.push_str(&format!("\ndetail: {detail}"));
    }
    if let Some(suggestion) = &error.suggestion {
        text.push_str(&format!("\nhint: {suggestion}"));
    }
    text
}

fn to_json(envelope: &Response) -> String {
    serde_json::to_string(envelope).unwrap_or_else(|error| {
        format!(
            r#"{{"version":"{}","ok":false,"command":"{}","error":{{"code":"INTERNAL","message":"{error}"}}}}"#,
            tabref_core::ENVELOPE_VERSION,
            envelope.command
        )
    })
}

#[cfg(test)]
#[path = "emit_tests.rs"]
mod tests;
