use super::control::Control;
use super::control_build::control_of;
use super::dom_tree::DomTree;
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;

const SIBLINGS_LOOKED_AT: usize = 3;

/// Whether the page hides the node at `index`: it or an ancestor has no
/// layout or is not visible, or it sits in a closed `<details>` outside
/// the `<summary>`.
pub(crate) fn is_hidden_at(tree: &DomTree, index: usize) -> bool {
    let chain = tree.ancestors(index);
    for (position, ancestor) in chain.iter().enumerate() {
        let Some(node) = tree.node(*ancestor) else {
            continue;
        };
        if node.kind == NodeKind::Document {
            break;
        }
        let laid_out = node.layout.is_some() || node.display() == "contents";
        let element = matches!(node.kind, NodeKind::Element(_));
        if element && node.layout.as_ref().is_some_and(|layout| !layout.visible) {
            return true;
        }
        if !laid_out && (element || position == 0) && !has_laid_out_descendant(tree, *ancestor) {
            return true;
        }
        if node.tag() == "DETAILS" && node.attr("open").is_none() {
            let through = position.checked_sub(1).and_then(|below| chain.get(below));
            let via_summary = through
                .and_then(|child| tree.node(*child))
                .is_some_and(|child| child.tag() == "SUMMARY");
            if !via_summary {
                return true;
            }
        }
    }
    false
}

fn has_laid_out_descendant(tree: &DomTree, index: usize) -> bool {
    let mut pending: Vec<usize> = tree.node(index).map(|node| node.children.clone()).unwrap_or_default();
    let mut seen = 0usize;
    while let Some(current) = pending.pop() {
        seen += 1;
        if seen > 2_000 {
            return false;
        }
        let Some(node) = tree.node(current) else {
            continue;
        };
        if node.layout.is_some() {
            return true;
        }
        pending.extend(node.children.iter().copied());
    }
    false
}

/// The control that would show the hidden node at `index`: the summary of
/// the closed `<details>` around it, an element whose `aria-controls`
/// names an element around it, or a collapsed control just before the
/// hidden part (an accordion's header).
pub(crate) fn expander_for(input: &OutlineInput, tree: &DomTree, index: usize) -> Option<Control> {
    let control_at = |at: usize| {
        let node = tree.node(at)?;
        let label = input.refs.get(&(tree.target_id.clone(), node.backend_id));
        control_of(tree, at, label)
    };
    let chain = tree.ancestors(index);
    for ancestor in &chain {
        let Some(node) = tree.node(*ancestor) else {
            continue;
        };
        if node.tag() == "DETAILS" && node.attr("open").is_none() {
            let summary = node
                .children
                .iter()
                .find(|child| tree.node(**child).is_some_and(|child| child.tag() == "SUMMARY"));
            if let Some(control) = summary.and_then(|summary| control_at(*summary)) {
                return Some(control);
            }
        }
        if let Some(id) = node.attr("id").filter(|id| !id.is_empty()) {
            let controller = tree.nodes.iter().position(|other| {
                other
                    .attr("aria-controls")
                    .is_some_and(|controls| controls.split_whitespace().any(|named| named == id))
            });
            if let Some(control) = controller.and_then(control_at) {
                return Some(control);
            }
        }
    }
    let outermost = chain
        .iter()
        .rev()
        .find(|ancestor| tree.node(**ancestor).is_some_and(|node| matches!(node.kind, NodeKind::Element(_)) && node.layout.is_none()))?;
    preceding_collapsed(tree, *outermost, &control_at)
}

fn preceding_collapsed(
    tree: &DomTree,
    hidden: usize,
    control_at: &dyn Fn(usize) -> Option<Control>,
) -> Option<Control> {
    let parent = tree.node(hidden)?.parent?;
    let siblings = &tree.node(parent)?.children;
    let place = siblings.iter().position(|sibling| *sibling == hidden)?;
    for sibling in siblings[..place].iter().rev().take(SIBLINGS_LOOKED_AT) {
        let mut pending = vec![*sibling];
        while let Some(current) = pending.pop() {
            let Some(node) = tree.node(current) else {
                continue;
            };
            if node.attr("aria-expanded") == Some("false") || node.tag() == "BUTTON" {
                if let Some(control) = control_at(current) {
                    return Some(control);
                }
            }
            pending.extend(node.children.iter().rev());
        }
    }
    None
}
