use serde::Serialize;
use serde_json::Value;

use super::error_payload::ErrorPayload;

/// The version of the `--json` envelope contract.
pub const ENVELOPE_VERSION: &str = "1.0";

/// The `--json` envelope: `{"version","ok","command","data"|"error"}`, printed
/// by every command.
#[derive(Debug, Serialize)]
pub struct Response {
    pub version: &'static str,
    pub ok: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

impl Response {
    pub fn ok(command: impl Into<String>, data: Value) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            ok: true,
            command: command.into(),
            data: Some(data),
            error: None,
        }
    }

    pub fn err(command: impl Into<String>, payload: ErrorPayload) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            ok: false,
            command: command.into(),
            data: None,
            error: Some(payload),
        }
    }
}

#[cfg(test)]
#[path = "envelope_tests.rs"]
mod tests;
