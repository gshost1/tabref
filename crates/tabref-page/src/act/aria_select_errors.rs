//! The errors `select` gives on an element that is not a native select.
//! Page text and the agent's choice are quoted as JSON strings, with
//! bidirectional marks dropped, so neither can break the line.

use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use super::option_match::{OptionEntry, OptionPick, listing, shown};
use super::select_option::refused;
use crate::ElementToken;

const BIDI: &[char] = &[
    '\u{200e}', '\u{200f}', '\u{061c}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
    '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
];
const MORE_MAY_LOAD: &str = "; more may load as the list scrolls or as you type";

/// `text` in double quotes on one line: `"`, `\` and control characters
/// escaped, bidirectional marks dropped.
pub(crate) fn quoted(text: &str) -> String {
    let kept: String = text.chars().filter(|c| !BIDI.contains(c)).collect();
    json!(kept).to_string()
}

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

/// A listbox that takes several options: a click there toggles one, so
/// `select` leaves it to clicks rather than guess what should stay chosen.
pub(crate) fn multi_select(token: &ElementToken) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!(
            "DOM node {} is a multi-select listbox, where a click toggles an option; click each option to change, then snap to check",
            token.backend_node_id
        ),
    )
    .with_details(json!({ "kind": "select_multi" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// No shown option is the one `choice` names. The options listed are the
/// ones the popup showed, which need not be all it has.
pub(crate) fn unpicked(options: &[OptionEntry], choice: &str, pick: &OptionPick) -> AppError {
    let mut error = refused(options, choice, pick);
    let all: Vec<usize> = (0..options.len()).collect();
    let list = listing(options, &all);
    error.message = match pick {
        OptionPick::Ambiguous(found) => format!(
            "{} matches {} options: {}; name one as listed",
            quoted(choice),
            found.len(),
            listing(options, found)
        ),
        OptionPick::Disabled(index) => format!(
            "option {} is disabled; shown options: {list}",
            quoted(&shown(&options[*index]))
        ),
        _ => format!(
            "no option matches {}; shown options: {list}{MORE_MAY_LOAD}",
            quoted(choice)
        ),
    };
    error
}

/// The popup showed no option at all, even after opening it (and typing
/// `choice` into an editable combobox).
pub(crate) fn no_options(element: &str, choice: &str, typed: bool) -> AppError {
    let tried = if typed {
        format!("clicking it and typing {}", quoted(choice))
    } else {
        "clicking it".to_owned()
    };
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "no option matches {}: {tried} showed no options in its listbox ({element}); snap to see the page",
            quoted(choice)
        ),
    )
    .with_details(json!({ "kind": "cdp_option_unavailable", "reason": "no_options", "options": [], "options_total": 0 }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

/// The option was clicked but gave no sign that it took.
pub(crate) fn not_taken(label: &str, shows: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "clicked option {}, but the control still shows {} and gave no sign it took the option; snap to see the page",
            quoted(label),
            quoted(&crate::cut_chars(shows, 80))
        ),
    )
    .with_details(json!({ "kind": "cdp_option_not_taken", "label": label, "shows": shows }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

/// The option matched a moment ago is not there with its label any more.
pub(crate) fn option_gone(label: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "the listbox's options changed while choosing (option {} moved or is gone); run it again",
            quoted(label)
        ),
    )
    .with_details(json!({ "kind": "cdp_option_changed" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// What an error adds after the text typed for the search was replaced by
/// the old text, or could not be.
pub(crate) fn put_back(previous: &str, restored: bool) -> String {
    if restored {
        format!(
            "; the typed text was replaced: put back {}",
            quoted(previous)
        )
    } else {
        format!(
            "; the typed text is still in the field: could not put back {}",
            quoted(previous)
        )
    }
}
