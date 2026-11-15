use serde_json::{Value, json};

use super::dom_display::collect_inline;
use super::dom_facts::DomFacts;
use super::dom_ids::{collect_clickable, collect_dom_ids};
use super::owner_box::collect_owner_boxes;
use super::owner_boxes::OwnerBoxes;
use crate::outline::dom_parse::CAPTURED_STYLES;

/// What one target's DOM capture gives a snapshot: the facts its tree build
/// reads, which nodes it holds and where it laid them out, for frame owners
/// (`None` when the capture had no layout to say), and the capture itself,
/// which the page view reads instead of taking a second one.
#[derive(Debug, Clone, Default)]
pub(crate) struct FrameDom {
    pub(crate) facts: DomFacts,
    pub(crate) boxes: Option<OwnerBoxes>,
    pub(crate) capture: Value,
}

impl FrameDom {
    /// The `DOMSnapshot.captureSnapshot` params, the page view's own:
    /// `display` first, as the inline-text reader expects, then `visibility`
    /// for the owner boxes, then `position`.
    pub(crate) fn capture_params() -> Value {
        json!({ "computedStyles": CAPTURED_STYLES })
    }

    pub(crate) fn from_capture(capture: Value) -> Self {
        Self {
            facts: DomFacts {
                ids: collect_dom_ids(&capture),
                clickable: collect_clickable(&capture),
                inline: collect_inline(&capture),
            },
            boxes: collect_owner_boxes(&capture),
            capture,
        }
    }
}

#[cfg(test)]
#[path = "frame_dom_tests.rs"]
mod tests;
