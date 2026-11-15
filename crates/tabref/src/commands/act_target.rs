use std::collections::HashSet;

use tabref_cdp::PageSession;
use tabref_core::{AccessibilityNode, AppError, Deadline};
use tabref_page::{
    ElementToken, LocateIntent, locate, snapshot_page_within_budget, wait_actionable,
};

use super::action_report::styled;
use super::target_spec::TargetSpec;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// The element an action runs on, however it was named: its token, the
/// session that addresses it, its tab, how the result names it, how an
/// error names it (with its role: `button "Save"`), and the element tokens
/// of the view it was found in, so the result can name the controls the
/// action made appear.
#[derive(Debug, Clone)]
pub(crate) struct ActTarget {
    pub(crate) token: ElementToken,
    pub(crate) session: PageSession,
    pub(crate) tab_id: String,
    pub(crate) label: String,
    pub(crate) subject: String,
    pub(crate) known: HashSet<String>,
}

impl ActTarget {
    /// Waits briefly for the element to be able to take the action `intent`
    /// names, and refuses it with the reason and the next step when it
    /// stays hidden, disabled or read-only, before any input is sent.
    pub(crate) fn ensure_ready(
        &self,
        session: &mut Session,
        intent: LocateIntent,
        deadline: Deadline,
    ) -> Result<(), AppError> {
        match wait_actionable(
            session.connection(),
            &self.session,
            &self.token,
            intent,
            deadline,
        )? {
            None => Ok(()),
            Some(blocked) => Err(blocked.error(&self.subject)),
        }
    }
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
            let (mut target, hidden) = match intent {
                LocateIntent::Read => (session.resolve_ref(text)?, false),
                _ => session.resolve_ref_or_hidden(text)?,
            };
            if hidden {
                let shown = wait_actionable(
                    session.connection(),
                    &target.session,
                    &target.token,
                    LocateIntent::Hover,
                    deadline,
                )?;
                if let Some(blocked) = shown {
                    return Err(blocked.error(&styled(&target.label())));
                }
                target = session.resolve_ref(text)?;
            }
            let label = styled(&target.label());
            Ok(ActTarget {
                subject: label.clone(),
                label,
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
            if let Some(scope) = &locator.scope {
                label.push_str(&format!(
                    " {} {}",
                    locator.scope_kind.keyword(),
                    quote_text(scope)
                ));
            }
            Ok(ActTarget {
                subject: format!("{} {label}", found.role),
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
