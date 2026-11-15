use serde::{Deserialize, Serialize};
use serde_json::Value;
use tabref_core::AppError;

use super::wire_error::WireError;
use crate::output::outcome::Outcome;

/// The daemon's one reply line: a command's outcome (text and `--json`
/// data) or its error. `replaced` says the daemon runs another build and is
/// exiting, so the client starts a new one and asks again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct WireReply {
    #[serde(default)]
    pub(crate) text: String,
    #[serde(default)]
    pub(crate) data: Value,
    #[serde(default)]
    pub(crate) error: Option<WireError>,
    #[serde(default)]
    pub(crate) replaced: bool,
}

impl WireReply {
    pub(crate) fn from_result(result: &Result<Outcome, AppError>) -> Self {
        match result {
            Ok(outcome) => Self {
                text: outcome.text.clone(),
                data: outcome.data.clone(),
                error: None,
                replaced: false,
            },
            Err(error) => Self {
                text: String::new(),
                data: Value::Null,
                error: Some(WireError::from(error)),
                replaced: false,
            },
        }
    }

    pub(crate) fn replaced() -> Self {
        Self {
            text: String::new(),
            data: Value::Null,
            error: None,
            replaced: true,
        }
    }

    pub(crate) fn into_result(self) -> Result<Outcome, AppError> {
        match self.error {
            Some(error) => Err(error.into()),
            None => Ok(Outcome::new(self.text, self.data)),
        }
    }
}
