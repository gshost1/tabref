use std::collections::HashSet;

use tabref_cdp::PageSession;
use tabref_core::{AccessibilityNode, AppError, Deadline};
use tabref_page::{ElementToken, LocateIntent, locate, snapshot_page_within_budget};

use super::target_spec::TargetSpec;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// The element an action runs on, however it was named: its token, the
/// session that addresses it, its tab, how the result names it, and the
/// element tokens of the view it was found in, so the result can name the
/// controls the action made appear.
#[derive(Debug, Clone)]
pub(crate) struct ActTarget {
    pub(crate) token: ElementToken,
    pub(crate) session: PageSession,
    pub(crate) tab_id: String,
    pub(crate) label: String,
    pub(crate) known: HashSet<String>,
}

/// Resolves `spec`: a ref through its snapshot, a locator against a fresh
/// read of the current tab. The read saves no refs, so the refs an agent
/// already holds keep naming what they named.
pub(crate) fn resolve_target(
    session: &mut Session,
    spec: &TargetSpec,
    intent: LocateIntent,
    deadline: Deadline,
) -> Result<ActTarget, AppError> {
    match spec {
        TargetSpec::Ref(text) => {
            let target = session.resolve_ref(text)?;
            Ok(ActTarget {
                label: target.label(),
                token: target.token,
                session: target.session,
                tab_id: target.tab_id,
                known: target.known.into_iter().collect(),
            })
        }
        TargetSpec::Find(locator) => {
            let page = session.page()?;
            let snapshot = snapshot_page_within_budget(session.connection(), &page, deadline)?;
            let found = locate(&snapshot.tree, locator, intent)?;
            let mut known = HashSet::new();
            collect_tokens(&snapshot.tree, &mut known);
            let target_session = session.attach(&found.token.target_id)?;
            let mut label = match &found.name {
                Some(name) => quote_text(name),
                None => found.role.clone(),
            };
            if locator.scope.is_some()
                && let Some(around) = &found.context
            {
                label.push_str(&format!(" in {}", quote_text(around)));
            }
            Ok(ActTarget {
                token: found.token,
                session: target_session,
                tab_id: page.target_id,
                label,
                known,
            })
        }
    }
}

fn collect_tokens(node: &AccessibilityNode, known: &mut HashSet<String>) {
    if let Some(token) = &node.identity.retained_object {
        known.insert(token.clone());
    }
    for child in &node.children {
        collect_tokens(child, known);
    }
}
