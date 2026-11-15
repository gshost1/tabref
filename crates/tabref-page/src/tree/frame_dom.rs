use serde_json::{Value, json};

use super::dom_display::{DISPLAY_STYLE, collect_inline};
use super::dom_facts::DomFacts;
use super::dom_ids::{collect_clickable, collect_dom_ids};
use super::owner_box::collect_owner_boxes;
use super::owner_boxes::OwnerBoxes;

const VISIBILITY_STYLE: &str = "visibility";

/// What one target's DOM capture gives a snapshot: the facts its tree build
/// reads, and which nodes it holds and where it laid them out, for frame
/// owners (`None` when the capture had no layout to say).
#[derive(Debug, Clone, Default)]
pub(crate) struct FrameDom {
    pub(crate) facts: DomFacts,
    pub(crate) boxes: Option<OwnerBoxes>,
}

impl FrameDom {
    /// The `DOMSnapshot.captureSnapshot` params: `display` first, as the
    /// inline-text reader expects, then `visibility` for the owner boxes.
    pub(crate) fn capture_params() -> Value {
        json!({ "computedStyles": [DISPLAY_STYLE, VISIBILITY_STYLE] })
    }

    pub(crate) fn from_capture(capture: &Value) -> Self {
        Self {
            facts: DomFacts {
                ids: collect_dom_ids(capture),
                clickable: collect_clickable(capture),
                inline: collect_inline(capture),
            },
            boxes: collect_owner_boxes(capture),
        }
    }
}

#[cfg(test)]
#[path = "frame_dom_tests.rs"]
mod tests;
