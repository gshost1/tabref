//! How `select` reads an ARIA control and reaches its parts: one read of
//! the control and its popup, a bounded poll of that read, the actionability
//! wait before each click, and the tokens of an option or of the text field
//! inside the control.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::action_need::ActionNeed;
use super::actionable::wait_actionable;
use super::aria_select_scripts::{FIELD, OPTION_AT, READ};
use super::element_call::ElementCall;
use super::element_errors::script_failed;
use super::option_match::OptionEntry;
use super::remote_element::{call_on_element, with_element};
use crate::{CdpConnection, ElementToken, PageSession};

const POLL: Duration = Duration::from_millis(50);
const LONGEST_WAIT: Duration = Duration::from_millis(1_500);

/// The control as one read saw it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Listed {
    pub kind: String,
    pub editable: bool,
    pub open: bool,
    pub multi: bool,
    pub focused: bool,
    pub options: Vec<Shown>,
    pub value: String,
    pub describe: String,
}

/// One option of the popup and whether it shows now.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shown {
    pub entry: OptionEntry,
    pub shown: bool,
    pub selected: bool,
}

/// The control a select works on, with what every call needs.
pub(crate) struct Control<'a> {
    pub connection: &'a mut CdpConnection,
    pub session: &'a PageSession,
    pub token: &'a ElementToken,
    pub deadline: Deadline,
}

impl Control<'_> {
    /// One read; `record` keeps the listboxes visible now, so a popup no
    /// attribute names is found as the one that appeared since.
    pub fn read(&mut self, record: bool) -> Result<Listed, AppError> {
        let read = call_on_element(
            self.connection,
            self.session,
            self.token,
            ElementCall {
                function: READ,
                arguments: vec![json!(record)],
            },
            self.deadline,
        )?;
        Ok(listed(&read))
    }

    /// Reads every 50 ms until `done` holds, for at most 1.5 s and half of
    /// what is left of the deadline, and returns the last read.
    pub fn wait_for(&mut self, done: impl Fn(&Listed) -> bool) -> Result<Listed, AppError> {
        let started = Instant::now();
        let budget = LONGEST_WAIT.min(self.deadline.remaining() / 2);
        loop {
            let state = self.read(false)?;
            if done(&state) || started.elapsed() + POLL >= budget {
                return Ok(state);
            }
            std::thread::sleep(POLL);
        }
    }

    /// Waits for `token` to take a click, as any click does.
    pub fn ready(&mut self, token: &ElementToken, subject: &str) -> Result<(), AppError> {
        match wait_actionable(
            self.connection,
            self.session,
            token,
            ActionNeed::Click,
            self.deadline,
        )? {
            None => Ok(()),
            Some(blocked) => Err(blocked.error(subject)),
        }
    }

    /// The option at `index` when its label is still `label`.
    pub fn option(&mut self, index: usize, label: &str) -> Result<Option<ElementToken>, AppError> {
        self.element(OPTION_AT, vec![json!(index), json!(label)])
    }

    /// The text field that takes typing: the one inside the control, or
    /// the control itself.
    pub fn field(&mut self) -> Result<ElementToken, AppError> {
        let found = self.element(FIELD, Vec::new())?;
        Ok(found.unwrap_or_else(|| self.token.clone()))
    }

    fn element(
        &mut self,
        function: &str,
        arguments: Vec<Value>,
    ) -> Result<Option<ElementToken>, AppError> {
        let (session, token, deadline) = (self.session, self.token, self.deadline);
        with_element(
            self.connection,
            session,
            token,
            deadline,
            |connection, object| {
                element_of(
                    connection,
                    session,
                    (object, function, arguments),
                    token,
                    deadline,
                )
            },
        )
    }
}

fn element_of(
    connection: &mut CdpConnection,
    session: &PageSession,
    (object_id, function, arguments): (&str, &str, Vec<Value>),
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Option<ElementToken>, AppError> {
    let arguments: Vec<Value> = arguments
        .into_iter()
        .map(|value| json!({ "value": value }))
        .collect();
    let reply = connection.call(
        "Runtime.callFunctionOn",
        json!({
            "objectId": object_id,
            "functionDeclaration": function,
            "arguments": arguments,
            "returnByValue": false,
            "awaitPromise": false,
        }),
        Some(&session.session_id),
        deadline,
    )?;
    if let Some(exception) = reply.get("exceptionDetails") {
        return Err(script_failed(exception));
    }
    let Some(found) = reply.pointer("/result/objectId").and_then(Value::as_str) else {
        return Ok(None);
    };
    let described = connection.call(
        "DOM.describeNode",
        json!({ "objectId": found }),
        Some(&session.session_id),
        deadline,
    );
    let _ = connection.call(
        "Runtime.releaseObject",
        json!({ "objectId": found }),
        Some(&session.session_id),
        deadline,
    );
    Ok(described?
        .pointer("/node/backendNodeId")
        .and_then(Value::as_i64)
        .map(|backend_node_id| ElementToken {
            backend_node_id,
            ..token.clone()
        }))
}

/// A read's JSON as a [`Listed`].
pub(crate) fn listed(read: &Value) -> Listed {
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let flag = |value: &Value, key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
    let options = read
        .get("options")
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .map(|option| Shown {
                    entry: OptionEntry {
                        label: text(option, "label"),
                        text: text(option, "text"),
                        value: text(option, "value"),
                        disabled: flag(option, "disabled"),
                    },
                    shown: flag(option, "shown"),
                    selected: flag(option, "selected"),
                })
                .collect()
        })
        .unwrap_or_default();
    Listed {
        kind: text(read, "kind"),
        editable: flag(read, "editable"),
        open: flag(read, "open"),
        multi: flag(read, "multi"),
        focused: flag(read, "focused"),
        options,
        value: text(read, "value"),
        describe: text(read, "describe"),
    }
}
