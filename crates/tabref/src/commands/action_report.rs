use serde_json::{Value, json};
use tabref_core::AppError;

use crate::output::outcome::Outcome;
use crate::output::page_report::page_report;
use crate::session::Session;
use crate::view::node_line::quote_text;
use crate::view::view_mode::ViewMode;

/// Waits for the action the session watched to settle and adds what it set
/// off to `outcome`: each dialog it opened, and, when it navigated or opened
/// a tab, a snapshot of the page it led to, so the next step needs no
/// separate `snap`.
pub(crate) fn finish_with_effects(
    session: &mut Session,
    mut outcome: Outcome,
) -> Result<Outcome, AppError> {
    let effects = session.finish_action()?;
    for dialog in &effects.dialogs {
        let kind = dialog
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("dialog");
        let message = dialog
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        outcome.push_line(&format!("dialog {kind} {} accepted", quote_text(message)));
    }
    set(&mut outcome.data, "dialogs", json!(effects.dialogs));
    set(&mut outcome.data, "navigated", json!(effects.navigated));
    if let Some(tab) = &effects.new_tab {
        set(&mut outcome.data, "new_tab", json!(tab));
        outcome.push_line("opened a new tab, now the current tab");
    }
    if effects.navigated || effects.new_tab.is_some() {
        let captured = session.capture()?;
        let report = page_report(&captured, ViewMode::Interactive);
        outcome.push_line(&report.text);
        set(&mut outcome.data, "page", report.data);
    }
    Ok(outcome)
}

pub(crate) fn set(data: &mut Value, key: &str, value: Value) {
    if let Some(object) = data.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
}
