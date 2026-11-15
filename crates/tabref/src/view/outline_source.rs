use std::collections::{BTreeMap, HashMap};

use tabref_core::capability::CLICK;
use tabref_core::{AccessibilityNode, AppError};
use tabref_page::{
    ElementToken, OutlineInput, RefLabel, capture_dom, frame_owner_in, read_viewport,
};

use crate::session::Session;

const FALLBACK_VIEWPORT: [f64; 2] = [1280.0, 800.0];
const INPUT_ROLES: &[&str] = &[
    "textfield",
    "combobox",
    "checkbox",
    "radiobutton",
    "listbox",
    "switch",
    "button",
];
const LARGE_FRAME_SHARE: f64 = 0.25;

/// Everything an outline of the current tab is built from: the DOM of the
/// page's own target, the DOM of each out-of-process iframe that holds a
/// field or a button with a ref or whose `<iframe>` covers a quarter of the
/// viewport (other cross-origin frames, such as ads, are counted but not
/// read), the viewport, and the refs of `tree` keyed by the target and
/// backend node id their element tokens name.
pub(crate) fn outline_input(
    session: &mut Session,
    tree: Option<&AccessibilityNode>,
) -> Result<OutlineInput, AppError> {
    let page = session.page()?;
    let deadline = session.deadline();
    let trees = capture_dom(session.connection(), &page, deadline)?;
    let viewport =
        read_viewport(session.connection(), &page, deadline).unwrap_or(FALLBACK_VIEWPORT);
    let mut refs = HashMap::new();
    let mut frame_targets = BTreeMap::new();
    if let Some(tree) = tree {
        collect_refs(tree, &mut refs, &mut frame_targets);
    }
    frame_targets.remove(&page.target_id);
    let mut frames = HashMap::new();
    for (target, asks_input) in frame_targets {
        let Ok(Some(owner)) = frame_owner_in(session.connection(), &page, &target, deadline) else {
            continue;
        };
        if !asks_input && !is_large(&trees, owner, viewport) {
            continue;
        }
        let Ok(child) = session.attach(&target) else {
            continue;
        };
        if let Ok(child_trees) = capture_dom(session.connection(), &child, deadline) {
            frames.insert((page.target_id.clone(), owner), child_trees);
        }
    }
    Ok(OutlineInput {
        trees,
        frames,
        refs,
        viewport,
    })
}

fn collect_refs(
    node: &AccessibilityNode,
    refs: &mut HashMap<(String, i64), RefLabel>,
    frame_targets: &mut BTreeMap<String, bool>,
) {
    if let (Some(ref_id), Some(token)) = (&node.ref_id, &node.identity.retained_object) {
        if let Ok(token) = ElementToken::parse(token) {
            let asks = INPUT_ROLES.contains(&node.role.as_str());
            let entry = frame_targets
                .entry(token.target_id.clone())
                .or_insert(false);
            *entry = *entry || asks;
            refs.insert(
                (token.target_id, token.backend_node_id),
                RefLabel {
                    ref_text: ref_id.clone(),
                    role: node.role.clone(),
                    name: node.display_name().map(str::to_owned),
                    value: node.identity.value.clone(),
                    states: node.presentation.states.clone(),
                    clickable: node.role != "cell"
                        && node
                            .presentation
                            .available_actions
                            .iter()
                            .any(|action| action == CLICK),
                },
            );
        }
    }
    for child in &node.children {
        collect_refs(child, refs, frame_targets);
    }
}

/// Whether the `<iframe>` with backend id `owner` covers a quarter of the
/// viewport.
fn is_large(trees: &[tabref_page::DomTree], owner: i64, viewport: [f64; 2]) -> bool {
    let area = viewport[0] * viewport[1];
    trees
        .iter()
        .filter_map(|tree| tree.find_backend(owner).and_then(|index| tree.node(index)))
        .filter_map(|node| node.layout.as_ref())
        .any(|layout| area > 0.0 && layout.area() >= LARGE_FRAME_SHARE * area)
}
