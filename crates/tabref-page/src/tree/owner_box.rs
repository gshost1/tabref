use std::collections::HashMap;

use serde_json::Value;

use super::frame_skip_reason::FrameSkipReason;

const FRAME_ELEMENTS: &[&str] = &["IFRAME", "FRAME"];
const TINY_EDGE: f64 = 2.0;

/// Where a DOM capture laid out a frame's owner element, in document
/// coordinates, and whether its computed `visibility` lets it show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OwnerBox {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) visible: bool,
}

impl OwnerBox {
    /// Why a frame in this box can show nothing, if it cannot.
    pub(crate) fn skip_reason(self) -> Option<FrameSkipReason> {
        if !self.visible {
            return Some(FrameSkipReason::Hidden);
        }
        if self.width <= 0.0 || self.height <= 0.0 {
            return Some(FrameSkipReason::ZeroSize);
        }
        if self.width <= TINY_EDGE && self.height <= TINY_EDGE {
            return Some(FrameSkipReason::Tiny);
        }
        if self.x + self.width <= 0.0 || self.y + self.height <= 0.0 {
            return Some(FrameSkipReason::Offscreen);
        }
        None
    }
}

/// The boxes of every `<iframe>` and `<frame>` element a capture laid out,
/// keyed by backend node id. The capture must have asked for `display` and
/// then `visibility`, so each layout object's `styles` holds those two.
/// `None` when the capture carries no layout at all, so nothing can be said
/// about an owner it does not list.
pub(crate) fn collect_owner_boxes(capture: &Value) -> Option<HashMap<i64, OwnerBox>> {
    let empty = Vec::new();
    let strings = capture.get("strings")?.as_array().unwrap_or(&empty);
    let string = |index: Option<&Value>| {
        index
            .and_then(Value::as_u64)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| strings.get(index))
            .and_then(Value::as_str)
    };
    let mut boxes = HashMap::new();
    let mut laid_out = false;
    for document in capture.get("documents")?.as_array()? {
        let column = |pointer: &str| document.pointer(pointer).and_then(Value::as_array);
        let (Some(names), Some(backends), Some(indexes), Some(bounds)) = (
            column("/nodes/nodeName"),
            column("/nodes/backendNodeId"),
            column("/layout/nodeIndex"),
            column("/layout/bounds"),
        ) else {
            continue;
        };
        laid_out = true;
        let styles = column("/layout/styles");
        for (position, node) in indexes.iter().enumerate() {
            let Some(node) = node.as_u64().and_then(|node| usize::try_from(node).ok()) else {
                continue;
            };
            let is_frame = string(names.get(node))
                .is_some_and(|name| FRAME_ELEMENTS.contains(&name.to_ascii_uppercase().as_str()));
            let (true, Some(backend)) = (is_frame, backends.get(node).and_then(Value::as_i64))
            else {
                continue;
            };
            let rect: Vec<f64> = bounds
                .get(position)
                .and_then(Value::as_array)
                .map(|rect| rect.iter().filter_map(Value::as_f64).collect())
                .unwrap_or_default();
            let visibility = styles
                .and_then(|styles| styles.get(position))
                .and_then(Value::as_array)
                .and_then(|values| string(values.get(1)));
            if let [x, y, width, height] = rect[..] {
                let visible = !matches!(visibility, Some("hidden" | "collapse"));
                boxes.insert(
                    backend,
                    OwnerBox {
                        x,
                        y,
                        width,
                        height,
                        visible,
                    },
                );
            }
        }
    }
    laid_out.then_some(boxes)
}

#[cfg(test)]
#[path = "owner_box_tests.rs"]
mod tests;
