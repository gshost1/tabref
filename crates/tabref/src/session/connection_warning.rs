//! The warning a connection tabref could not vouch for carries to every
//! command that uses it, success or error: a `warning:` line before the
//! command's text (or its error), and `warnings` in the JSON envelope's
//! `data` (or the error's `details`). A kept daemon connection stays
//! unverified, so it warns on each command, not only the first.

use serde_json::{Map, Value, json};
use tabref_core::AppError;

use super::Session;
use crate::output::outcome::Outcome;

impl Session {
    /// `outcome` with the connection's warning, when it has one.
    pub(crate) fn with_warning(
        &self,
        outcome: Result<Outcome, AppError>,
    ) -> Result<Outcome, AppError> {
        warned(outcome, self.warning.as_deref())
    }
}

/// `outcome`, success or error, with `warning` added when there is one.
pub(crate) fn warned(
    outcome: Result<Outcome, AppError>,
    warning: Option<&str>,
) -> Result<Outcome, AppError> {
    let Some(warning) = warning else {
        return outcome;
    };
    match outcome {
        Ok(mut outcome) => {
            add_warning(&mut outcome, warning);
            Ok(outcome)
        }
        Err(error) => {
            let mut details = match error.details.clone() {
                Some(Value::Object(details)) => details,
                _ => Map::new(),
            };
            push_warning(&mut details, warning);
            Err(error.with_details(Value::Object(details)))
        }
    }
}

pub(crate) fn add_warning(outcome: &mut Outcome, warning: &str) {
    outcome.text = if outcome.text.is_empty() {
        format!("warning: {warning}")
    } else {
        format!("warning: {warning}\n{}", outcome.text)
    };
    if let Value::Object(data) = &mut outcome.data {
        push_warning(data, warning);
    }
}

fn push_warning(data: &mut Map<String, Value>, warning: &str) {
    match data.get_mut("warnings") {
        Some(Value::Array(list)) => list.push(json!(warning)),
        _ => {
            data.insert("warnings".to_owned(), json!([warning]));
        }
    }
}

#[cfg(test)]
#[path = "connection_warning_tests.rs"]
mod tests;
