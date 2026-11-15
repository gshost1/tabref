use serde_json::json;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use super::blocker::Blocker;

/// An element that stayed unable to take its action for as long as the
/// action waited: why, the page's own reason (`display:none`,
/// `aria-disabled`), and how long it was waited for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unactionable {
    pub blocker: Blocker,
    pub detail: String,
    pub waited_ms: u64,
}

impl Unactionable {
    /// The `ACTION_FAILED` error for `subject` (`button "Save"`): one line
    /// with the reason and the next step, and a disposition that says no
    /// input was sent.
    pub fn error(&self, subject: &str) -> AppError {
        let message = match self.blocker {
            Blocker::Hidden => format!(
                "{subject} is hidden ({}); open the section, tab or menu that holds it first",
                self.detail
            ),
            Blocker::Disabled => {
                let why = match self.detail.as_str() {
                    "disabled" => String::new(),
                    other => format!(" ({other})"),
                };
                format!("{subject} is disabled{why}; it may need a field filled first")
            }
            Blocker::ReadOnly => format!(
                "{subject} is read-only; the page sets it, so change the control that fills it"
            ),
        };
        AppError::new(ErrorCode::ActionFailed, message)
            .with_details(json!({
                "kind": self.blocker.kind(),
                "reason": self.detail,
                "waited_ms": self.waited_ms,
            }))
            .with_disposition(DeliverySemantics::not_delivered())
    }
}

#[cfg(test)]
#[path = "unactionable_tests.rs"]
mod tests;
