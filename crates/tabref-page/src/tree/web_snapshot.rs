use std::collections::HashMap;

use serde_json::Value;
use tabref_core::AccessibilityNode;

use super::skipped_frame::SkippedFrame;
use crate::NodeOrigin;

/// A page's accessibility tree in core form. `origins[i]` belongs to the
/// node at depth-first pre-order index `i` of `tree` (the root is 0); it is
/// `None` for a node with no DOM backing. Every node that can receive a ref
/// also carries its element token in `identity.retained_object`, so the
/// side table is a convenience. `skipped_frames` lists every frame left out
/// of the tree and why; `unreadable_frames` holds the ids of those that were
/// left out unread (timed out or refused), whose content is unknown.
#[derive(Debug, Clone)]
pub struct WebSnapshot {
    pub tree: AccessibilityNode,
    pub origins: Vec<Option<NodeOrigin>>,
    pub unreadable_frames: Vec<String>,
    pub skipped_frames: Vec<SkippedFrame>,
    /// The raw `DOMSnapshot.captureSnapshot` of each target whose frames
    /// are in the tree, by target id, for a DOM view to reuse.
    pub dom_captures: HashMap<String, Value>,
}

impl WebSnapshot {
    /// Every node in depth-first pre-order beside its origin.
    pub fn preorder(&self) -> Vec<(&AccessibilityNode, Option<&NodeOrigin>)> {
        let mut nodes = Vec::with_capacity(self.origins.len());
        let mut pending = vec![&self.tree];
        while let Some(node) = pending.pop() {
            let origin = self.origins.get(nodes.len()).and_then(Option::as_ref);
            nodes.push((node, origin));
            pending.extend(node.children.iter().rev());
        }
        nodes
    }
}
