use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use crate::{CdpConnection, PageSession};

/// Evaluates `expression` in the page's main frame and returns its value as
/// JSON. A promise is awaited. A value JSON cannot carry, such as a DOM node
/// or a function, comes back as its description string, and `undefined` as
/// `null`. A thrown exception is `ACTION_FAILED` with the exception text.
pub fn evaluate(
    connection: &mut CdpConnection,
    session: &PageSession,
    expression: &str,
    deadline: Deadline,
) -> Result<Value, AppError> {
    let mut reply = connection.call(
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
            "userGesture": true,
            "replMode": true,
        }),
        Some(&session.session_id),
        deadline,
    )?;
    if let Some(exception) = reply.get("exceptionDetails") {
        return Err(thrown(exception));
    }
    let result = reply.get_mut("result").map_or(Value::Null, Value::take);
    if let Some(value) = result.get("value") {
        return Ok(value.clone());
    }
    if let Some(unserializable) = result.get("unserializableValue") {
        return Ok(unserializable.clone());
    }
    Ok(result.get("description").cloned().unwrap_or(Value::Null))
}

fn thrown(exception: &Value) -> AppError {
    let text = exception
        .pointer("/exception/description")
        .or_else(|| exception.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("script exception");
    AppError::new(ErrorCode::ActionFailed, "The expression threw an exception")
        .with_platform_detail(text)
        .with_details(json!({ "kind": "js_exception" }))
        .with_disposition(DeliverySemantics::delivered_unverified())
}

#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod tests;
