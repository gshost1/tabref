use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::element_call::ElementCall;
use super::option_match::{
    LISTED_CHARS, OptionEntry, OptionPick, listed_name, listing, pick_option, shown,
};
use super::remote_element::call_on_element;
use super::select_outcome::SelectOutcome;
use crate::{CdpConnection, ElementToken, PageSession, cut_chars};

#[path = "combobox.rs"]
pub(super) mod combobox;

const DETAILED: usize = 30;

const READ_OPTIONS: &str = r"function() {
  if (this.localName !== 'select') {
    const role = (this.getAttribute('role') || '').trim().split(/\s+/)[0];
    return role === 'combobox' ? { combobox: true } : { error: 'not_select' };
  }
  return { options: Array.from(this.options).map(o => ({
    label: o.label, text: o.text, value: o.value,
    disabled: o.disabled || (o.parentElement && o.parentElement.localName === 'optgroup' && o.parentElement.disabled),
  })) };
}";

const CHOOSE_OPTION: &str = r"function(index, value) {
  const match = this.options[index];
  if (!match || match.value !== value) return { error: 'changed' };
  const chosen = this.selectedIndex >= 0 ? this.options[this.selectedIndex] : null;
  const previous = chosen ? chosen.label.trim() : null;
  if (this.multiple) { match.selected = true; } else { this.selectedIndex = index; }
  const View = this.ownerDocument.defaultView;
  this.dispatchEvent(new View.Event('input', { bubbles: true }));
  this.dispatchEvent(new View.Event('change', { bubbles: true }));
  return { value: this.value, label: match.label.trim(), previous };
}";

/// Selects the option that `choice` names. Opens an ARIA combobox with
/// pointer input and clicks the option. For a native select, matches the
/// options as printed by tabref, sets the value, and fires input and change.
pub fn select_option(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    choice: &str,
    deadline: Deadline,
) -> Result<SelectOutcome, AppError> {
    let read = call_on_element(
        connection,
        session,
        token,
        ElementCall {
            function: READ_OPTIONS,
            arguments: Vec::new(),
        },
        deadline,
    )?;
    if read["combobox"] == true {
        return combobox::choose(connection, session, token, choice, deadline);
    }
    if read.get("error").is_some() {
        return Err(AppError::new(
            ErrorCode::ActionNotSupported,
            format!("DOM node {} is not a <select>", token.backend_node_id),
        )
        .with_disposition(DeliverySemantics::not_delivered()));
    }
    let options = entries(read.get("options"));
    let index = match pick_option(&options, choice) {
        OptionPick::Chosen(index) => index,
        other => return Err(refused(&options, choice, &other)),
    };
    let outcome = call_on_element(
        connection,
        session,
        token,
        ElementCall {
            function: CHOOSE_OPTION,
            arguments: vec![json!(index), json!(options[index].value)],
        },
        deadline,
    )?;
    if outcome.get("error").is_some() {
        return Err(AppError::new(
            ErrorCode::ActionFailed,
            format!("the select's options changed while choosing '{choice}'; run it again"),
        )
        .with_details(json!({ "kind": "cdp_option_changed" }))
        .with_disposition(DeliverySemantics::not_delivered()));
    }
    let text = |key: &str| outcome.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(SelectOutcome {
        value: text("value").unwrap_or_default(),
        label: text("label").unwrap_or_default(),
        previous: text("previous"),
        combobox: false,
    })
}

fn entries(options: Option<&Value>) -> Vec<OptionEntry> {
    let field = |option: &Value, key: &str| {
        option
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    options
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .map(|option| OptionEntry {
                    label: field(option, "label"),
                    text: field(option, "text"),
                    value: field(option, "value"),
                    disabled: option
                        .get("disabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn refused(options: &[OptionEntry], choice: &str, pick: &OptionPick) -> AppError {
    let all: Vec<usize> = (0..options.len()).collect();
    let named = |indices: &[usize]| -> Vec<String> {
        indices
            .iter()
            .take(DETAILED)
            .map(|index| listed_name(&options[*index]))
            .collect()
    };
    let labels = |indices: &[usize]| -> Vec<String> {
        indices
            .iter()
            .map(|index| cut_chars(options[*index].label.trim(), LISTED_CHARS))
            .collect()
    };
    let legacy_options = match pick {
        OptionPick::Ambiguous(found) => Some(labels(found)),
        OptionPick::Disabled(_) => None,
        _ => Some(labels(&all[..all.len().min(DETAILED)])),
    };
    let (code, kind, reason, message, matches) = match pick {
        OptionPick::Ambiguous(found) => (
            ErrorCode::AmbiguousTarget,
            "cdp_option_ambiguous",
            "ambiguous",
            format!(
                "'{choice}' matches {} options: {}; name one as listed",
                found.len(),
                listing(options, found)
            ),
            named(found),
        ),
        OptionPick::Disabled(index) => (
            ErrorCode::ActionFailed,
            "cdp_option_unavailable",
            "disabled_option",
            format!(
                "option '{}' is disabled; options: {}",
                shown(&options[*index]),
                listing(options, &all)
            ),
            named(&[*index]),
        ),
        _ => (
            ErrorCode::ActionFailed,
            "cdp_option_unavailable",
            "no_option",
            format!(
                "no option matches '{choice}'; options: {}",
                listing(options, &all)
            ),
            Vec::new(),
        ),
    };
    AppError::new(code, message)
        .with_details(json!({
            "kind": kind,
            "reason": reason,
            "matches": matches,
            "options": legacy_options,
            "all_options": named(&all),
            "options_total": options.len(),
        }))
        .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "select_option_tests.rs"]
mod tests;
