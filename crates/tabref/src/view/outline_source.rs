use std::collections::{BTreeMap, HashMap};

use tabref_core::capability::CLICK;
use tabref_core::{AccessibilityNode, AppError};
use tabref_page::{
    DomTree, ElementToken, OutlineInput, RefLabel, capture_dom, frame_owner_in, read_viewport,
};

use crate::session::Session;
use crate::session::captured_snapshot::CapturedSnapshot;

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
/// read), the viewport, and the refs of the snapshot's tree keyed by the
/// target and backend node id their element tokens name. The DOM of a
/// target the snapshot already captured is read from that capture; a frame
/// the snapshot left out is not captured again, so a stalled one costs the
/// view nothing.
pub(crate) fn outline_input(
    session: &mut Session,
    captured: Option<&CapturedSnapshot>,
) -> Result<OutlineInput, AppError> {
    let page = session.page()?;
    let deadline = session.deadline();
    let tree = captured.map(|captured| &captured.tree);
    let trees = match captured.and_then(|captured| captured.dom(&page.target_id)) {
        Some(trees) => trees.to_vec(),
        None => capture_dom(session.connection(), &page, deadline)?,
    };
    let viewport =
        read_viewport(session.connection(), &page, deadline).unwrap_or(FALLBACK_VIEWPORT);
    let mut refs = HashMap::new();
    let mut frame_targets = BTreeMap::new();
    if let Some(tree) = tree {
        collect_refs(tree, &mut refs, &mut frame_targets);
    }
    if tree.is_none() {
        for target in iframe_targets(session) {
            frame_targets.entry(target).or_insert(false);
        }
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
        if let Some(child_trees) = captured.and_then(|captured| captured.dom(&target)) {
            frames.insert((page.target_id.clone(), owner), child_trees.to_vec());
            continue;
        }
        if captured.is_some_and(|captured| left_out(captured, &target)) {
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

/// Whether the snapshot left `target` out, whatever the reason.
fn left_out(captured: &CapturedSnapshot, target: &str) -> bool {
    captured
        .skipped_frames
        .iter()
        .any(|frame| frame.frame_id == target)
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
fn is_large(trees: &[DomTree], owner: i64, viewport: [f64; 2]) -> bool {
    let area = viewport[0] * viewport[1];
    trees
        .iter()
        .filter_map(|tree| tree.find_backend(owner).and_then(|index| tree.node(index)))
        .filter_map(|node| node.layout.as_ref())
        .any(|layout| area > 0.0 && layout.area() >= LARGE_FRAME_SHARE * area)
}

/// The browser's out-of-process iframe targets, for a view taken without a
/// snapshot, which has no refs to name them. Which page holds each one is
/// settled by asking the page for its owner element.
fn iframe_targets(session: &mut Session) -> Vec<String> {
    let deadline = session.deadline();
    let Ok(reply) =
        session
            .connection()
            .call("Target.getTargets", serde_json::json!({}), None, deadline)
    else {
        return Vec::new();
    };
    reply
        .get("targetInfos")
        .and_then(serde_json::Value::as_array)
        .map(|targets| {
            targets
                .iter()
                .filter(|target| {
                    target.get("type").and_then(serde_json::Value::as_str) == Some("iframe")
                })
                .filter_map(|target| target.get("targetId").and_then(serde_json::Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
