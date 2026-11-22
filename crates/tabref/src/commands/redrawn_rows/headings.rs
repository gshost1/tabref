use std::collections::{HashMap, HashSet};

use tabref_core::AccessibilityNode;
use tabref_page::{DomNode, DomTree, subtree_text};

use super::{location, role, useful_label};

pub(super) type ControlKey<'a> = (&'a str, Option<&'a str>);
type DocumentPositions<'a> = HashMap<(&'a str, &'a str), HashSet<usize>>;

pub(super) fn control_key(node: &AccessibilityNode) -> ControlKey<'_> {
    (&node.role, node.display_name())
}

pub(super) fn ref_nodes(root: &AccessibilityNode) -> impl Iterator<Item = &AccessibilityNode> {
    let mut pending = vec![root];
    std::iter::from_fn(move || {
        let node = pending.pop()?;
        pending.extend(&node.children);
        Some(node)
    })
    .filter(|node| node.ref_id.is_some())
}

pub(super) enum HeadingLabel {
    NoScope,
    InScope(Option<String>),
}

pub(super) struct HeadingControls<'n, 'd> {
    positions: HashMap<ControlKey<'n>, DocumentPositions<'d>>,
}

impl<'n, 'd> HeadingControls<'n, 'd> {
    pub(super) fn new(
        reported: &[&AccessibilityNode],
        controls: impl IntoIterator<Item = &'n AccessibilityNode>,
        documents: &impl Fn(&str) -> Option<&'d [DomTree]>,
    ) -> Self {
        let keys: HashSet<_> = reported.iter().map(|node| control_key(node)).collect();
        let mut positions = HashMap::<_, DocumentPositions<'d>>::new();
        if keys.is_empty() {
            return Self { positions };
        }
        for control in controls {
            let key = control_key(control);
            if keys.contains(&key)
                && let Some((doc, index)) = location(control, documents)
            {
                positions
                    .entry(key)
                    .or_default()
                    .entry((&doc.target_id, &doc.frame_id))
                    .or_default()
                    .insert(index);
            }
        }
        Self { positions }
    }

    pub(super) fn label(
        &self,
        control: &AccessibilityNode,
        doc: &DomTree,
        container: &DomNode,
        index: usize,
    ) -> HeadingLabel {
        let key = control_key(control);
        let peers = self
            .positions
            .get(&key)
            .and_then(|docs| docs.get(&(doc.target_id.as_str(), doc.frame_id.as_str())));
        let mut order = Vec::new();
        let mut headings = Vec::new();
        let mut pending: Vec<_> = container.children.iter().rev().copied().collect();
        while let Some(at) = pending.pop() {
            let Some(node) = doc.node(at) else {
                continue;
            };
            if at == index || peers.is_some_and(|peers| peers.contains(&at)) && node.is_shown() {
                order.push(Landmark::Control(at));
                continue;
            }
            if is_heading(node) {
                headings.push(at);
                order.push(Landmark::Heading(at));
            }
            pending.extend(node.children.iter().rev().copied());
        }
        let label = |heading| {
            let text = subtree_text(doc, heading);
            useful_label(&text, key.1.unwrap_or_default().trim()).then_some(text)
        };
        match headings.as_slice() {
            [] => HeadingLabel::NoScope,
            [only] => label(*only).map_or(HeadingLabel::NoScope, |text| {
                HeadingLabel::InScope(Some(text))
            }),
            _ => HeadingLabel::InScope(paired_heading(&order, index).and_then(label)),
        }
    }
}

fn is_heading(node: &DomNode) -> bool {
    node.is_shown()
        && (matches!(node.tag(), "H1" | "H2" | "H3" | "H4" | "H5" | "H6")
            || role(node) == Some("heading"))
}

enum Landmark {
    Heading(usize),
    Control(usize),
}

fn paired_heading(order: &[Landmark], index: usize) -> Option<usize> {
    use Landmark::{Control, Heading};

    let control_first = matches!(order.first(), Some(Control(_)));
    let mut matched = None;
    for pair in order.chunks(2) {
        let (control, heading) = match (control_first, pair) {
            (true, [Control(control), Heading(heading)])
            | (false, [Heading(heading), Control(control)]) => (*control, *heading),
            _ => return None,
        };
        if control == index {
            matched = Some(heading);
        }
    }
    matched
}
