use crate::AccessibilityNode;
use crate::tree::capability;
use crate::tree::roles::is_interactive_role;

const UBIQUITOUS_AFFORDANCES: [&str; 3] = [
    capability::SET_FOCUS,
    capability::RIGHT_CLICK,
    capability::SCROLL_TO,
];

/// A node receives a ref when an agent can act on it: its role is
/// interactive, or it advertises an action other than focus, context menu or
/// scroll-into-view, which a page offers on nearly every node and which would
/// give every inert container a ref.
pub fn is_ref_able(node: &AccessibilityNode) -> bool {
    is_ref_able_role_actions(&node.role, &node.presentation.available_actions)
}

pub fn is_ref_able_role_actions(role: &str, available_actions: &[String]) -> bool {
    is_interactive_role(role)
        || available_actions
            .iter()
            .any(|action| !UBIQUITOUS_AFFORDANCES.contains(&action.as_str()))
}
