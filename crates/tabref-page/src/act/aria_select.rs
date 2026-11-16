//! `select` on the ARIA patterns a page builds instead of a `<select>`: a
//! combobox (select-only or editable), a button with `aria-haspopup=listbox`,
//! or a listbox. It does what a user does: opens the popup when it is closed
//! (a click, then Down Arrow), waits for its options, types the wanted text
//! into an editable combobox when no shown option matches, matches the
//! option as a native select's is matched, clicks it, and reads the control
//! back. Only shown options are matched: one a filter hid cannot be clicked.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics};

use super::action_need::ActionNeed;
use super::actionable::wait_actionable;
use super::aria_select_errors::{no_options, not_taken, unpicked, unsupported};
use super::aria_select_scripts::{FIELD, OPTION_AT, READ};
use super::click::click_times;
use super::element_call::ElementCall;
use super::element_errors::script_failed;
use super::fill::fill;
use super::key_press::press_key;
use super::option_match::{OptionEntry, OptionPick, pick_option};
use super::remote_element::{call_on_element, with_element};
use super::select_outcome::SelectOutcome;
use crate::{CdpConnection, ElementToken, PageSession};

const POLL: Duration = Duration::from_millis(50);
const LONGEST_WAIT: Duration = Duration::from_millis(1_500);

/// The control as one read saw it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Listed {
    pub kind: String,
    pub editable: bool,
    pub open: bool,
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

/// Picks the option `choice` names in the ARIA control `token` names.
pub(crate) fn select_listed(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    choice: &str,
    deadline: Deadline,
) -> Result<SelectOutcome, AppError> {
    let mut page = Control {
        connection,
        session,
        token,
        deadline,
    };
    let mut state = page.read(true)?;
    if state.kind == "none" {
        return Err(unsupported(token, &state.describe));
    }
    let previous = state.value.clone();
    let mut opened = false;
    if !state.open {
        page.ready(token, &format!("{} {}", state.kind, state.describe))?;
        click_times(page.connection, session, token, 1, deadline)?;
        opened = true;
        state = page.wait_for(|now| now.open)?;
        if !state.open && !state.editable && state.kind != "listbox" {
            press_key(page.connection, session, "ArrowDown", deadline)?;
            state = page.wait_for(|now| now.open)?;
        }
    }
    let mut pick = pick_shown(&state, choice).0;
    let mut typed = false;
    if state.editable && pick == OptionPick::Missing {
        let field = page.field()?;
        fill(page.connection, session, &field, choice, deadline)?;
        typed = true;
        opened = true;
        state =
            page.wait_for(|now| now.open && pick_shown(now, choice).0 != OptionPick::Missing)?;
        pick = pick_shown(&state, choice).0;
    }
    let OptionPick::Chosen(at) = pick else {
        if opened {
            let _ = press_key(page.connection, session, "Escape", deadline);
        }
        let error = if state.open || state.options.iter().any(|option| option.shown) {
            let (entries, _) = shown_entries(&state);
            unpicked(&entries, choice, &pick, typed)
        } else {
            no_options(&state.describe, choice, typed)
        };
        return Err(if opened {
            error.with_disposition(DeliverySemantics::delivered_unverified())
        } else {
            error
        });
    };
    let index = shown_entries(&state).1[at];
    let label = state.options[index].entry.label.clone();
    let option = page.option(index)?;
    page.ready(&option, &format!("option \"{label}\""))?;
    click_times(page.connection, session, &option, 1, deadline)?;
    let after = page.wait_for(|now| took(now, index, &label, &previous))?;
    if !took(&after, index, &label, &previous) {
        return Err(not_taken(&label, &after.value));
    }
    Ok(SelectOutcome {
        value: after.value,
        label,
        previous: (!previous.is_empty()).then_some(previous),
        read_back: true,
    })
}

/// Whether the control now shows `label`: its displayed value is the
/// label, or holds it and changed, or, for a listbox, the option is
/// selected.
fn took(state: &Listed, index: usize, label: &str, previous: &str) -> bool {
    let value = state.value.to_lowercase();
    let wanted = label.to_lowercase();
    if state.kind == "listbox" {
        return state
            .options
            .get(index)
            .is_some_and(|option| option.selected);
    }
    value == wanted || (value.contains(&wanted) && state.value != previous)
}

/// The shown options as entries, and the index of each in all options.
fn shown_entries(state: &Listed) -> (Vec<OptionEntry>, Vec<usize>) {
    state
        .options
        .iter()
        .enumerate()
        .filter(|(_, option)| option.shown)
        .map(|(index, option)| (option.entry.clone(), index))
        .unzip()
}

fn pick_shown(state: &Listed, choice: &str) -> (OptionPick, Vec<usize>) {
    let (entries, indices) = shown_entries(state);
    (pick_option(&entries, choice), indices)
}

struct Control<'a> {
    connection: &'a mut CdpConnection,
    session: &'a PageSession,
    token: &'a ElementToken,
    deadline: Deadline,
}

impl Control<'_> {
    fn read(&mut self, record: bool) -> Result<Listed, AppError> {
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

    fn wait_for(&mut self, done: impl Fn(&Listed) -> bool) -> Result<Listed, AppError> {
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

    fn ready(&mut self, token: &ElementToken, subject: &str) -> Result<(), AppError> {
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

    fn option(&mut self, index: usize) -> Result<ElementToken, AppError> {
        let found = self.element(OPTION_AT, vec![json!(index)])?;
        found.ok_or_else(|| super::aria_select_errors::option_gone(index))
    }

    fn field(&mut self) -> Result<ElementToken, AppError> {
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

fn listed(read: &Value) -> Listed {
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
        options,
        value: text(read, "value"),
        describe: text(read, "describe"),
    }
}

#[cfg(test)]
#[path = "aria_select_tests.rs"]
mod tests;
