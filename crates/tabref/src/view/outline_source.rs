use std::collections::{BTreeSet, HashMap};

use tabref_core::capability::CLICK;
use tabref_core::{AccessibilityNode, AppError};
use tabref_page::{
    ElementToken, OutlineInput, RefLabel, capture_dom, frame_owner_in, read_viewport,
};

use crate::session::Session;

const FALLBACK_VIEWPORT: [f64; 2] = [1280.0, 800.0];
const FIELD_ROLES: &[&str] = &[
    "textfield",
    "combobox",
    "checkbox",
    "radiobutton",
    "listbox",
    "switch",
];

/// Everything an outline of the current tab is built from: the DOM of the
/// page's own target, the DOM of each out-of-process iframe that holds a
/// field to fill in (other cross-origin frames, such as ads, are counted
/// but not read), the viewport, and the refs of `tree` keyed by the target
/// and backend node id their element tokens name.
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
    let mut field_targets = BTreeSet::new();
    if let Some(tree) = tree {
        collect_refs(tree, &mut refs, &mut field_targets);
    }
    field_targets.remove(&page.target_id);
    let mut frames = HashMap::new();
    for target in field_targets {
        let Ok(child) = session.attach(&target) else {
            continue;
        };
        let Ok(Some(owner)) = frame_owner_in(session.connection(), &page, &target, deadline) else {
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
    field_targets: &mut BTreeSet<String>,
) {
    if let (Some(ref_id), Some(token)) = (&node.ref_id, &node.identity.retained_object) {
        if let Ok(token) = ElementToken::parse(token) {
            if FIELD_ROLES.contains(&node.role.as_str()) {
                field_targets.insert(token.target_id.clone());
            }
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
        collect_refs(child, refs, field_targets);
    }
}
