use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use crate::act::element_errors::script_failed;
use crate::act::remote_element::with_element;
use crate::{CdpConnection, ElementToken, PageSession};

/// What a scroll moves: the page, or the container that holds an element.
/// Each script a scroll runs gets the site as `this`: the page's `window`,
/// or the element, whose own document and window the script then uses.
#[derive(Debug, Clone, Copy)]
pub enum ScrollSite<'a> {
    Page,
    Element(&'a ElementToken),
}

/// A script for a [`ScrollSite`]: the function's source and the JSON
/// arguments to pass it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SiteScript<'s> {
    pub(crate) function: &'s str,
    pub(crate) arguments: &'s [Value],
}

impl ScrollSite<'_> {
    /// Runs `function` with the site as `this` and returns its JSON value,
    /// awaiting a promise it returns. The script is tabref's own, never a
    /// user gesture.
    pub(crate) fn call(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        script: SiteScript<'_>,
        deadline: Deadline,
    ) -> Result<Value, AppError> {
        let mut reply = self.run(connection, session, script, deadline)?;
        Ok(reply
            .pointer_mut("/result/value")
            .map_or(Value::Null, Value::take))
    }

    fn run(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        script: SiteScript<'_>,
        deadline: Deadline,
    ) -> Result<Value, AppError> {
        let SiteScript {
            function,
            arguments,
        } = script;
        let reply = match self {
            Self::Page => {
                let listed: Vec<String> = arguments.iter().map(Value::to_string).collect();
                let expression = format!("({function}).call(window, {})", listed.join(", "));
                connection.call(
                    "Runtime.evaluate",
                    json!({
                        "expression": expression,
                        "returnByValue": true,
                        "awaitPromise": true,
                    }),
                    Some(&session.session_id),
                    deadline,
                )?
            }
            Self::Element(token) => with_element(
                connection,
                session,
                token,
                deadline,
                |connection, object| {
                    let arguments: Vec<Value> = arguments
                        .iter()
                        .map(|value| json!({ "value": value }))
                        .collect();
                    connection.call(
                        "Runtime.callFunctionOn",
                        json!({
                        "objectId": object,
                        "functionDeclaration": function,
                        "arguments": arguments,
                        "returnByValue": true,
                        "awaitPromise": true,
                        }),
                        Some(&session.session_id),
                        deadline,
                    )
                },
            )?,
        };
        if let Some(exception) = reply.get("exceptionDetails") {
            return Err(script_failed(exception));
        }
        Ok(reply)
    }
}
