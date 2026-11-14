use serde_json::{Value, json};
use tabref_core::AppError;
use tabref_page::evaluate;

use crate::output::outcome::Outcome;
use crate::session::Session;

/// Evaluates `expression` in the current tab and prints its value: a string
/// as it is, anything else as JSON.
pub(crate) fn run(session: &mut Session, expression: &str) -> Result<Outcome, AppError> {
    let page = session.page()?;
    let deadline = session.deadline();
    let value = evaluate(session.connection(), &page, expression, deadline)?;
    let text = match &value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    Ok(Outcome::new(text, json!({ "value": value })))
}
