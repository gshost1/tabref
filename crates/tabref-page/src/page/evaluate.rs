use serde_json::{Value, json};
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use crate::{CdpConnection, PageSession};

const SCRIPT_GROUP: &str = "tabref-js";
const AS_VALUE: &str = "function() { return this; }";
const RELEASE_MS: u64 = 500;

/// Evaluates `expression` in the page's main frame and returns its value as
/// JSON. A promise is awaited, whether the expression awaits it or returns
/// it, within `deadline`: one that rejects is `ACTION_FAILED` with the
/// reason, and one still pending at the deadline is `TIMEOUT`. A value JSON
/// cannot carry, such as a function, comes back as its description string,
/// and `undefined` as `null`. A thrown exception is `ACTION_FAILED` with
/// the exception text. The expression runs as a user gesture, so it may
/// open a window or start media as the agent's own script.
pub fn evaluate(
    connection: &mut CdpConnection,
    session: &PageSession,
    expression: &str,
    deadline: Deadline,
) -> Result<Value, AppError> {
    evaluate_settled(connection, session, expression, deadline).map_err(|error| {
        if is_timeout(&error) {
            unsettled()
        } else {
            error
        }
    })
}

fn evaluate_settled(
    connection: &mut CdpConnection,
    session: &PageSession,
    expression: &str,
    deadline: Deadline,
) -> Result<Value, AppError> {
    let session_id = Some(session.session_id.as_str());
    let mut reply = connection.call(
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": false,
            "awaitPromise": true,
            "userGesture": true,
            "replMode": true,
            "objectGroup": SCRIPT_GROUP,
        }),
        session_id,
        deadline,
    )?;
    let outcome = if let Some(exception) = reply.get("exceptionDetails") {
        Err(evaluation_error(
            connection, session_id, expression, exception, deadline,
        ))
    } else {
        let result = reply.get_mut("result").map_or(Value::Null, Value::take);
        let Some(object_id) = result.get("objectId").and_then(Value::as_str) else {
            return Ok(plain(result));
        };
        object_value(connection, session, &result, object_id, deadline)
    };
    let release = Deadline::after(RELEASE_MS).unwrap_or(deadline);
    let _ = connection.call(
        "Runtime.releaseObjectGroup",
        json!({ "objectGroup": SCRIPT_GROUP }),
        session_id,
        release,
    );
    outcome
}

fn evaluation_error(
    connection: &mut CdpConnection,
    session_id: Option<&str>,
    expression: &str,
    exception: &Value,
    deadline: Deadline,
) -> AppError {
    let text = exception["text"].as_str().unwrap_or_default();
    let in_promise = text.starts_with("Uncaught (in promise)")
        || (expression.contains("await")
            && exception
                .pointer("/exception/className")
                .and_then(Value::as_str)
                != Some("SyntaxError")
            && requires_top_level_await(connection, session_id, expression, deadline));
    if in_promise {
        rejected(exception)
    } else {
        thrown(exception)
    }
}

/// Chrome's REPL reports a rejected implicit async evaluation as
/// "Uncaught", also used for synchronous throws. Check whether the source
/// requires REPL-mode await syntax by compiling it as an ordinary script.
/// This never runs the expression a second time.
fn requires_top_level_await(
    connection: &mut CdpConnection,
    session_id: Option<&str>,
    expression: &str,
    deadline: Deadline,
) -> bool {
    if connection
        .call("Runtime.enable", json!({}), session_id, deadline)
        .is_err()
    {
        return false;
    }
    let Ok(compiled) = connection.call(
        "Runtime.compileScript",
        json!({ "expression": expression, "sourceURL": "", "persistScript": false }),
        session_id,
        deadline,
    ) else {
        return false;
    };
    let error = &compiled["exceptionDetails"];
    let description = error
        .pointer("/exception/description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if description.starts_with("SyntaxError: await is only valid") {
        return true;
    }
    // Chrome points at the 'await' token for a top-level for-await loop.
    description.starts_with("SyntaxError: Unexpected reserved word")
        && error["lineNumber"]
            .as_u64()
            .zip(error["columnNumber"].as_u64())
            .is_some_and(|(line, column)| {
                expression.lines().nth(line as usize).is_some_and(|line| {
                    line.encode_utf16()
                        .skip(column as usize)
                        .take(5)
                        .eq("await".encode_utf16())
                })
            })
}

/// The value of the object a script returned: what a promise settles to,
/// or the object itself serialized as JSON.
fn object_value(
    connection: &mut CdpConnection,
    session: &PageSession,
    result: &Value,
    object_id: &str,
    deadline: Deadline,
) -> Result<Value, AppError> {
    let session_id = Some(session.session_id.as_str());
    if result.get("subtype").and_then(Value::as_str) == Some("promise") {
        let mut reply = connection.call(
            "Runtime.awaitPromise",
            json!({ "promiseObjectId": object_id, "returnByValue": true }),
            session_id,
            deadline,
        )?;
        if let Some(exception) = reply.get("exceptionDetails") {
            return Err(rejected(exception));
        }
        return Ok(plain(
            reply.get_mut("result").map_or(Value::Null, Value::take),
        ));
    }
    let mut reply = connection.call(
        "Runtime.callFunctionOn",
        json!({ "objectId": object_id, "functionDeclaration": AS_VALUE, "returnByValue": true }),
        session_id,
        deadline,
    )?;
    if reply.get("exceptionDetails").is_some() {
        return Ok(plain(result.clone()));
    }
    Ok(plain(
        reply.get_mut("result").map_or(Value::Null, Value::take),
    ))
}

fn plain(result: Value) -> Value {
    if let Some(value) = result.get("value") {
        return value.clone();
    }
    if let Some(unserializable) = result.get("unserializableValue") {
        return unserializable.clone();
    }
    result.get("description").cloned().unwrap_or(Value::Null)
}

fn unsettled() -> AppError {
    AppError::timeout("The expression or its promise did not settle before the command's timeout")
        .with_details(json!({ "kind": "js_pending" }))
        .with_disposition(DeliverySemantics::delivered_unverified())
}

fn rejected(exception: &Value) -> AppError {
    let reason = exception
        .pointer("/exception/description")
        .or_else(|| exception.pointer("/exception/value"))
        .or_else(|| exception.get("text"))
        .map(|reason| match reason {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| "no reason given".to_owned());
    AppError::new(ErrorCode::ActionFailed, "The expression's promise rejected")
        .with_platform_detail(reason)
        .with_details(json!({ "kind": "js_rejected" }))
        .with_disposition(DeliverySemantics::delivered_unverified())
}

/// Evaluates tabref's own reads and scrolls, which must not count as a user
/// gesture a page could act on. A promise the expression awaits is awaited;
/// one it returns is not.
pub(crate) fn evaluate_quietly(
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
            "userGesture": false,
            "replMode": true,
        }),
        Some(&session.session_id),
        deadline,
    )?;
    if let Some(exception) = reply.get("exceptionDetails") {
        return Err(thrown(exception));
    }
    Ok(plain(
        reply.get_mut("result").map_or(Value::Null, Value::take),
    ))
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
