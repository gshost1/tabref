use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::append_outcome::AppendOutcome;
use super::element_call::ElementCall;
use super::element_errors::not_editable;
use super::key_press::press_key;
use super::remote_element::{call_function, with_element};
use crate::{CdpConnection, ElementToken, PageSession};

const PREPARE_APPEND: &str = r"function() {
  const TEXT_TYPES = ['text', 'search', 'url', 'tel', 'email', 'password'];
  const VALUE_TYPES = ['date', 'time', 'datetime-local', 'month', 'week', 'color', 'range', 'number'];
  const tag = this.localName;
  if (tag === 'input' && VALUE_TYPES.includes(this.type)) return { whole: this.type };
  const root = this.getRootNode ? this.getRootNode() : null;
  const host = root && root !== this ? root.host : null;
  if (host && host.localName === 'input' && VALUE_TYPES.includes(host.type)) return { whole: host.type };
  const control = tag === 'textarea' || (tag === 'input' && TEXT_TYPES.includes(this.type));
  if (tag === 'input' && !control) return { editable: false, reason: 'input type ' + this.type };
  if (control && (this.disabled || this.readOnly)) return { editable: false, reason: this.disabled ? 'disabled' : 'read-only' };
  if (!control && !this.isContentEditable) return { editable: false, reason: 'not editable' };
  this.focus();
  const active = (root && root.activeElement) || this.ownerDocument.activeElement;
  const focused = active === this || (!control && !!active && (active.contains(this) || this.contains(active)));
  if (!focused) return { editable: false, reason: 'did not take focus' };
  if (control) {
    const before = String(this.value);
    try {
      this.setSelectionRange(before.length, before.length);
      return { editable: true, before, caret: 'set' };
    } catch (error) {
      return { editable: true, before, caret: 'end-key' };
    }
  }
  const doc = this.ownerDocument;
  const range = doc.createRange();
  range.selectNodeContents(this);
  range.collapse(false);
  const selection = doc.getSelection();
  selection.removeAllRanges();
  selection.addRange(range);
  const shown = this.innerText !== undefined ? this.innerText : this.textContent;
  return { editable: true, before: shown === null ? '' : String(shown), caret: 'set' };
}";

const READ_APPENDED: &str = r"function() {
  if (this.localName === 'input' || this.localName === 'textarea') return String(this.value);
  const text = this.innerText !== undefined ? this.innerText : this.textContent;
  return text === null || text === undefined ? '' : String(text);
}";

/// Types `text` after the node's current content, as one input edit, and
/// reads the whole content back on both sides. Only a text field takes it:
/// a text-like `<input>`, a `<textarea>` or a contenteditable element; a
/// button, checkbox, select, link, or a disabled or read-only field is
/// `ACTION_NOT_SUPPORTED` before anything is typed, and so is a node that
/// does not take focus, so the text never lands in another field. An input
/// that takes its value whole (number, date, time, ...) is refused with the
/// advice to use `fill`. The caret is moved past the last character; an
/// input with no selection API (`email`) gets an End key press instead.
pub fn append_text(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    text: &str,
    deadline: Deadline,
) -> Result<AppendOutcome, AppError> {
    with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            let prepared = call_function(
                connection,
                session,
                object,
                script(PREPARE_APPEND),
                deadline,
            )?;
            if let Some(kind) = prepared.get("whole").and_then(Value::as_str) {
                return Err(whole_value(token.backend_node_id, kind));
            }
            if prepared.get("editable").and_then(Value::as_bool) != Some(true) {
                let reason = prepared.get("reason").and_then(Value::as_str);
                return Err(not_editable(token.backend_node_id, reason));
            }
            if prepared.get("caret").and_then(Value::as_str) == Some("end-key") {
                press_key(connection, session, "End", deadline)?;
            }
            if !text.is_empty() {
                connection.call(
                    "Input.insertText",
                    json!({ "text": text }),
                    Some(&session.session_id),
                    deadline,
                )?;
            }
            let observed =
                call_function(connection, session, object, script(READ_APPENDED), deadline)?
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
            let previous = prepared
                .get("before")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Ok(AppendOutcome {
                verified: appended(&previous, text, &observed),
                previous,
                observed,
            })
        },
    )
}

/// Whether `observed` is `previous` with `text` after it, reading a
/// non-breaking space (an editor stores a typed space as one) as a space
/// and ignoring trailing line breaks.
pub(crate) fn appended(previous: &str, text: &str, observed: &str) -> bool {
    let observed = normal(observed);
    observed == normal(&format!("{previous}{text}"))
        || observed == format!("{}{}", normal(previous), normal(text))
}

fn normal(text: &str) -> String {
    text.replace('\u{a0}', " ")
        .trim_end_matches('\n')
        .to_owned()
}

fn script(function: &'static str) -> ElementCall {
    ElementCall {
        function,
        arguments: Vec::new(),
    }
}

fn whole_value(backend_node_id: i64, input_type: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!("type adds text to a text field; use fill for a number/date input (this is a {input_type} input)"),
    )
    .with_details(json!({
        "kind": "cdp_whole_value_input",
        "backend_node_id": backend_node_id,
        "input_type": input_type,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "append_text_tests.rs"]
mod tests;
