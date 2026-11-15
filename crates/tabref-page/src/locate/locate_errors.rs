use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use super::locate_intent::LocateIntent;
use super::locator::Locator;

/// `ELEMENT_NOT_FOUND` naming up to three of the closest labels.
pub(crate) fn not_found(locator: &Locator, intent: LocateIntent, closest: &[String]) -> AppError {
    let mut message = format!("no {} matches {}", intent.noun(), locator.describe());
    if closest.is_empty() {
        message.push_str("; nothing similar here, run 'tabref snap' to see the page");
    } else {
        message.push_str(&format!("; closest: {}", closest.join(", ")));
    }
    AppError::new(ErrorCode::ElementNotFound, message)
        .with_details(json!({ "kind": "locate_not_found", "closest": closest }))
        .with_disposition(DeliverySemantics::not_delivered())
}

/// `AMBIGUOUS_TARGET` listing up to three of the matches.
pub(crate) fn ambiguous(locator: &Locator, labels: &[String], count: usize) -> AppError {
    let shown: Vec<&String> = labels.iter().take(3).collect();
    let listed = shown
        .iter()
        .map(|label| label.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    AppError::new(
        ErrorCode::AmbiguousTarget,
        format!(
            "{count} elements match {}: {listed}; add nth N or in \"<text near it>\"",
            locator.describe()
        ),
    )
    .with_details(json!({ "kind": "locate_ambiguous", "matches": labels, "count": count }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// `ELEMENT_NOT_FOUND` for an `in "<text>"` that no node shows.
pub(crate) fn no_scope(scope: &str) -> AppError {
    AppError::new(
        ErrorCode::ElementNotFound,
        format!("no text {scope:?} on the page to look near"),
    )
    .with_details(json!({ "kind": "locate_scope_not_found" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// `ELEMENT_NOT_FOUND` for an `nth N` past the last match.
pub(crate) fn past_last(locator: &Locator, count: usize) -> AppError {
    AppError::new(
        ErrorCode::ElementNotFound,
        format!("{} asks past the {count} match(es)", locator.describe()),
    )
    .with_details(json!({ "kind": "locate_nth_out_of_range", "count": count }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// `ELEMENT_NOT_FOUND` for an `in "<text>"` whose row or record holds no
/// match: the text is there, the control is not, and no neighbour's stands
/// in for it.
pub(crate) fn not_in_record(locator: &Locator, intent: LocateIntent) -> AppError {
    let scope = locator.scope.as_deref().unwrap_or_default();
    AppError::new(
        ErrorCode::ElementNotFound,
        format!(
            "the row or record holding {scope:?} has no {} matching {}",
            intent.noun(),
            locator.describe()
        ),
    )
    .with_details(json!({ "kind": "locate_not_in_record" }))
    .with_disposition(DeliverySemantics::not_delivered())
}
