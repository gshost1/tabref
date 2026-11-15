use super::block::Block;
use super::dom_node::DomNode;
use super::dom_tree::DomTree;
use super::element_kind::{is_dialog_markup, role_of};
use super::item::Item;
use super::text_runs::{cut_chars, squash, subtree_text};

const MIN_OVERLAY_TEXT: usize = 40;
const LABEL_LIMIT: usize = 80;

/// Whether an element may be a dialog the view lifts to its top: a dialog
/// by markup, or a fixed-position box covering at least half the viewport
/// that is not navigation (a modal sheet, a consent wall). A sticky bar is
/// not one. Whether it stays a dialog is decided once its content is read.
pub(crate) fn is_dialog_candidate(node: &DomNode, viewport: [f64; 2]) -> bool {
    if is_dialog_markup(node) {
        return node.is_shown();
    }
    let Some(layout) = node.layout.as_ref().filter(|layout| layout.visible) else {
        return false;
    };
    let [width, height] = viewport;
    if layout.position != "fixed" || width <= 0.0 || height <= 0.0 {
        return false;
    }
    let navigation = matches!(node.tag(), "NAV" | "HEADER")
        || matches!(role_of(node).as_deref(), Some("navigation" | "banner"));
    !navigation && layout.bounds[2] >= 0.4 * width && layout.area() >= 0.5 * width * height
}

/// Whether a candidate read into `block` is kept as a dialog: markup says
/// so, or it holds a button and a sentence of text.
pub(crate) fn keeps_as_dialog(node: &DomNode, block: &Block) -> bool {
    if is_dialog_markup(node) {
        return !block.is_empty();
    }
    let mut text = 0usize;
    let mut buttons = 0usize;
    count(block, &mut text, &mut buttons);
    buttons > 0 && text >= MIN_OVERLAY_TEXT
}

fn count(block: &Block, text: &mut usize, buttons: &mut usize) {
    for item in &block.items {
        match item {
            Item::Text(run) => *text += run.trim().chars().count(),
            Item::Control(control) if control.role == "button" => *buttons += 1,
            Item::Block(inner) => count(inner, text, buttons),
            _ => {}
        }
    }
}

/// A landmark's or dialog's name: `aria-label`, the text `aria-labelledby`
/// points at, or, for a dialog, its first heading.
pub(crate) fn dialog_label(tree: &DomTree, index: usize) -> Option<String> {
    let node = tree.node(index)?;
    if let Some(label) = node
        .attr("aria-label")
        .map(squash)
        .filter(|label| !label.is_empty())
    {
        return Some(cut_chars(&label, LABEL_LIMIT));
    }
    if let Some(ids) = node.attr("aria-labelledby") {
        let text: Vec<String> = ids
            .split_whitespace()
            .filter_map(|id| {
                tree.nodes
                    .iter()
                    .position(|other| other.attr("id") == Some(id))
            })
            .map(|labelled| subtree_text(tree, labelled))
            .filter(|text| !text.is_empty())
            .collect();
        if !text.is_empty() {
            return Some(cut_chars(&text.join(" "), LABEL_LIMIT));
        }
    }
    if !is_dialog_markup(node)
        && node
            .layout
            .as_ref()
            .is_none_or(|layout| layout.position != "fixed")
    {
        return None;
    }
    first_heading(tree, index).map(|heading| cut_chars(&heading, LABEL_LIMIT))
}

fn first_heading(tree: &DomTree, index: usize) -> Option<String> {
    let mut pending = vec![index];
    while let Some(current) = pending.pop() {
        let node = tree.node(current)?;
        if matches!(node.tag(), "H1" | "H2" | "H3" | "H4" | "H5" | "H6") && node.is_shown() {
            let text = subtree_text(tree, current);
            if !text.is_empty() {
                return Some(text);
            }
        }
        pending.extend(node.children.iter().rev());
    }
    None
}
