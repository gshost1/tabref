use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::element_call::ElementCall;
use super::remote_element::call_on_element;
use super::select_outcome::SelectOutcome;
use crate::{CdpConnection, ElementToken, PageSession};

const SELECT_OPTION: &str = r"function(choice) {
  if (this.localName !== 'select') return { error: 'not_select' };
  const options = Array.from(this.options);
  const wanted = String(choice).trim();
  const low = wanted.toLowerCase();
  const label = o => o.label.trim();
  const text = o => o.text.trim();
  const tiers = [
    o => label(o) === wanted || o.value === wanted || text(o) === wanted,
    o => label(o).toLowerCase() === low || o.value.toLowerCase() === low || text(o).toLowerCase() === low,
    o => low !== '' && (label(o).toLowerCase().startsWith(low) || text(o).toLowerCase().startsWith(low)),
  ];
  let match = null;
  for (const test of tiers) {
    const found = options.filter(test);
    if (found.length === 1) { match = found[0]; break; }
    if (found.length > 1) return { error: 'ambiguous', options: found.map(label) };
  }
  if (!match) return { error: 'no_option', options: options.map(label) };
  if (match.disabled) return { error: 'disabled_option' };
  const chosen = this.selectedIndex >= 0 ? this.options[this.selectedIndex] : null;
  const previous = chosen ? label(chosen) : null;
  if (this.multiple) { match.selected = true; } else { this.selectedIndex = match.index; }
  const View = this.ownerDocument.defaultView;
  this.dispatchEvent(new View.Event('input', { bubbles: true }));
  this.dispatchEvent(new View.Event('change', { bubbles: true }));
  return { value: this.value, label: label(match), previous };
}";

/// Selects the `<option>` of a `<select>` that `choice` names: its label,
/// value or text exactly, then the same ignoring case, then the one option
/// whose label starts with `choice`. Several options at the first rank
/// that matches are `AMBIGUOUS_TARGET`. Fires `input` and `change` as a
/// user's pick would; a native select's popup cannot be driven with page
/// input, so the value is set directly.
pub fn select_option(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    choice: &str,
    deadline: Deadline,
) -> Result<SelectOutcome, AppError> {
    let outcome = call_on_element(
        connection,
        session,
        token,
        ElementCall {
            function: SELECT_OPTION,
            arguments: vec![json!(choice)],
        },
        deadline,
    )?;
    match outcome.get("error").and_then(Value::as_str) {
        None => {
            let text = |key: &str| outcome.get(key).and_then(Value::as_str).map(str::to_owned);
            Ok(SelectOutcome {
                value: text("value").unwrap_or_default(),
                label: text("label").unwrap_or_default(),
                previous: text("previous"),
            })
        }
        Some("not_select") => Err(AppError::new(
            ErrorCode::ActionNotSupported,
            format!("DOM node {} is not a <select>", token.backend_node_id),
        )
        .with_disposition(DeliverySemantics::not_delivered())),
        Some("ambiguous") => Err(AppError::new(
            ErrorCode::AmbiguousTarget,
            format!(
                "'{choice}' matches several options: {}",
                option_list(outcome.get("options"))
            ),
        )
        .with_details(json!({
            "kind": "cdp_option_ambiguous",
            "options": outcome.get("options").cloned().unwrap_or(Value::Null),
        }))
        .with_disposition(DeliverySemantics::not_delivered())),
        Some(reason) => Err(AppError::new(
            ErrorCode::ActionFailed,
            format!(
                "No selectable option matches '{choice}'; options: {}",
                option_list(outcome.get("options"))
            ),
        )
        .with_details(json!({
            "kind": "cdp_option_unavailable",
            "reason": reason,
            "options": outcome.get("options").cloned().unwrap_or(Value::Null),
        }))
        .with_disposition(DeliverySemantics::not_delivered())),
    }
}

fn option_list(options: Option<&Value>) -> String {
    let labels: Vec<&str> = options
        .and_then(Value::as_array)
        .map(|options| options.iter().filter_map(Value::as_str).take(12).collect())
        .unwrap_or_default();
    labels.join(", ")
}

#[cfg(test)]
#[path = "select_option_tests.rs"]
mod tests;
