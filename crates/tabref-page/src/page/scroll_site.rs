use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use crate::act::element_errors::script_failed;
use crate::act::remote_element::with_element;
use crate::{CdpConnection, ElementToken, PageSession};

const SCROLL_GROUP: &str = "tabref-scroll";

/// What a scroll moves: the page, or the container that holds an element.
/// Each script a scroll runs gets the site as `this`: the page's `window`,
/// or the element, whose own document and window the script then uses.
#[derive(Debug, Clone, Copy)]
pub enum ScrollSite<'a> {
    Page,
    Element(&'a ElementToken),
}

impl ScrollSite<'_> {
    /// Runs `function` with the site as `this` and returns its JSON value,
    /// awaiting a promise it returns. The script is tabref's own, never a
    /// user gesture.
    pub(crate) fn call(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        function: &str,
        arguments: &[Value],
        deadline: Deadline,
    ) -> Result<Value, AppError> {
        let mut reply = self.run(connection, session, function, arguments, true, deadline)?;
        Ok(reply
            .pointer_mut("/result/value")
            .map_or(Value::Null, Value::take))
    }

    /// Runs `function` as [`call`](Self::call) does and keeps the object it
    /// returns, for a later [`release`](Self::release). `None` when it
    /// returned no object.
    pub(crate) fn object(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        function: &str,
        arguments: &[Value],
        deadline: Deadline,
    ) -> Result<Option<String>, AppError> {
        let reply = self.run(connection, session, function, arguments, false, deadline)?;
        Ok(reply
            .pointer("/result/objectId")
            .and_then(Value::as_str)
            .map(str::to_owned))
    }

    /// Releases the objects [`object`](Self::object) kept.
    pub(crate) fn release(
        connection: &mut CdpConnection,
        session: &PageSession,
        deadline: Deadline,
    ) {
        let _ = connection.call(
            "Runtime.releaseObjectGroup",
            json!({ "objectGroup": SCROLL_GROUP }),
            Some(&session.session_id),
            deadline,
        );
    }

    fn run(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        function: &str,
        arguments: &[Value],
        by_value: bool,
        deadline: Deadline,
    ) -> Result<Value, AppError> {
        let reply = match self {
            Self::Page => {
                let listed: Vec<String> = arguments.iter().map(Value::to_string).collect();
                let expression = format!("({function}).call(window, {})", listed.join(", "));
                connection.call(
                    "Runtime.evaluate",
                    json!({
                        "expression": expression,
                        "returnByValue": by_value,
                        "awaitPromise": true,
                        "objectGroup": SCROLL_GROUP,
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
                            "returnByValue": by_value,
                            "awaitPromise": true,
                            "objectGroup": SCROLL_GROUP,
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
