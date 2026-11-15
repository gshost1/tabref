//! The warning a connection carries to the first command that uses it: a
//! `warning:` line before the command's text, and `data.warnings` in the
//! JSON envelope. A kept daemon connection warns once, not on every command.

use serde_json::{Value, json};
use tabref_core::AppError;

use super::Session;
use crate::output::outcome::Outcome;

impl Session {
    /// `outcome` with the connection's warning, if it has one not yet shown.
    pub(crate) fn with_warning(
        &mut self,
        outcome: Result<Outcome, AppError>,
    ) -> Result<Outcome, AppError> {
        let Ok(mut outcome) = outcome else {
            return outcome;
        };
        if let Some(warning) = self.warning.take() {
            add_warning(&mut outcome, &warning);
        }
        Ok(outcome)
    }
}

pub(crate) fn add_warning(outcome: &mut Outcome, warning: &str) {
    outcome.text = if outcome.text.is_empty() {
        format!("warning: {warning}")
    } else {
        format!("warning: {warning}\n{}", outcome.text)
    };
    if let Value::Object(data) = &mut outcome.data {
        match data.get_mut("warnings") {
            Some(Value::Array(list)) => list.push(json!(warning)),
            _ => {
                data.insert("warnings".to_owned(), json!([warning]));
            }
        }
    }
}

#[cfg(test)]
#[path = "connection_warning_tests.rs"]
mod tests;
