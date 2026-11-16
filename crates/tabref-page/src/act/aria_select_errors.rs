//! The errors `select` gives on an element that is not a native select.

use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use super::option_match::{OptionEntry, OptionPick};
use super::select_option::refused;
use crate::ElementToken;

/// An element that is neither a `<select>` nor an ARIA combobox, listbox or
/// listbox button: tabref does not guess how a custom widget picks. The
/// command names the element; `element` says what it is.
pub(crate) fn unsupported(token: &ElementToken, element: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!(
            "DOM node {} is a {element}, not a <select> or an ARIA combobox or listbox; click it, then click the option",
            token.backend_node_id
        ),
    )
    .with_details(json!({ "kind": "select_unsupported", "element": element }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// No shown option is the one `choice` names: the native select's refusal,
/// saying when the text was typed into the combobox first.
pub(crate) fn unpicked(
    options: &[OptionEntry],
    choice: &str,
    pick: &OptionPick,
    typed: bool,
) -> AppError {
    let mut error = refused(options, choice, pick);
    if typed {
        error
            .message
            .push_str(&format!("; '{choice}' was typed into the combobox"));
    }
    error
}

/// The popup showed no option at all, even after opening it (and typing
/// `choice` into an editable combobox).
pub(crate) fn no_options(element: &str, choice: &str, typed: bool) -> AppError {
    let tried = if typed {
        format!("clicking it and typing '{choice}'")
    } else {
        "clicking it and pressing ArrowDown".to_owned()
    };
    AppError::new(
        ErrorCode::ActionFailed,
        format!("no option matches '{choice}': {tried} showed no options in its listbox ({element}); snap to see the page"),
    )
    .with_details(json!({ "kind": "cdp_option_unavailable", "reason": "no_options", "options": [], "options_total": 0 }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

/// The option was clicked but the control does not show it.
pub(crate) fn not_taken(label: &str, shows: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("clicked option \"{label}\", but the control still shows \"{shows}\"; snap to see the page"),
    )
    .with_details(json!({ "kind": "cdp_option_not_taken", "label": label, "shows": shows }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

/// The option matched a moment ago is gone from the popup.
pub(crate) fn option_gone(index: usize) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "the listbox's options changed while choosing (option {index} is gone); run it again"
        ),
    )
    .with_details(json!({ "kind": "cdp_option_changed" }))
    .with_disposition(DeliverySemantics::not_delivered())
}
