use tabref_core::{AccessibilityNode, AppError};

use crate::output::outcome::Outcome;
use crate::output::page_report::{page_report, subtree_report};
use crate::session::Session;
use crate::view::view_mode::ViewMode;

/// Takes a new snapshot of the current tab. With `root`, prints only the
/// subtree of that element, read fresh, with refs of the new snapshot.
pub(crate) fn run(
    session: &mut Session,
    all: bool,
    root: Option<&str>,
) -> Result<Outcome, AppError> {
    let mode = ViewMode::from_all(all);
    let Some(root) = root else {
        let captured = session.capture()?;
        return Ok(page_report(&captured, mode));
    };
    let target = session.resolve_ref(root)?;
    let captured = session.capture()?;
    let token = target.token.to_string();
    let subtree = find_subtree(&captured.tree, &token).ok_or_else(|| {
        AppError::stale_ref_because(format!("{} is no longer in the page", target.ref_id))
            .with_details(serde_json::json!({ "kind": "cdp_node_gone" }))
    })?;
    Ok(subtree_report(&captured, subtree, mode))
}

fn find_subtree<'a>(node: &'a AccessibilityNode, token: &str) -> Option<&'a AccessibilityNode> {
    if node.identity.retained_object.as_deref() == Some(token) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| find_subtree(child, token))
}
