use super::dom_tree::DomTree;
use super::node_kind::NodeKind;
use super::walk_context::WalkContext;

/// For every node of every captured document, whether it or anything
/// inside it is rendered and visible. An element with no box of its own
/// (`display: contents`, or `visibility: hidden` around a visible child) is
/// still walked when something inside it shows; an element nothing inside
/// of shows is skipped whole.
pub(crate) struct ShownMap {
    shown: Vec<Vec<Vec<bool>>>,
}

impl ShownMap {
    pub(crate) fn new(captures: &[&[DomTree]]) -> Self {
        let shown = captures
            .iter()
            .map(|documents| documents.iter().map(shown_in).collect())
            .collect();
        Self { shown }
    }

    pub(crate) fn has_shown(&self, context: &WalkContext, index: usize) -> bool {
        self.shown
            .get(context.capture)
            .and_then(|documents| documents.get(context.document))
            .and_then(|nodes| nodes.get(index))
            .copied()
            .unwrap_or(false)
    }
}

fn shown_in(tree: &DomTree) -> Vec<bool> {
    let mut shown: Vec<bool> = tree
        .nodes
        .iter()
        .map(|node| match node.kind {
            NodeKind::Text(ref text) => node.is_shown() && !text.trim().is_empty(),
            NodeKind::Element(ref tag) => {
                node.is_shown()
                    && (matches!(tag.as_str(), "INPUT" | "SELECT" | "TEXTAREA" | "BUTTON" | "IFRAME" | "FRAME")
                        || node.attr("href").is_some()
                        || node.attr("aria-label").is_some())
            }
            _ => false,
        })
        .collect();
    for index in (0..tree.nodes.len()).rev() {
        if !shown[index] {
            continue;
        }
        if let Some(parent) = tree.nodes[index].parent.filter(|parent| *parent < index) {
            shown[parent] = true;
        }
    }
    shown
}
