use serde_json::json;
use tabref_cdp::PageSession;
use tabref_core::{AppError, RefEntry, RefId};
use tabref_page::{ElementToken, read_node_identity};

use super::Session;

/// A ref resolved to a live element: the ref, what its snapshot recorded,
/// the element token, the session that addresses the element (its tab's, or
/// an out-of-process iframe's), and the tab it belongs to.
#[derive(Debug, Clone)]
pub(crate) struct RefTarget {
    pub(crate) ref_id: RefId,
    pub(crate) entry: RefEntry,
    pub(crate) token: ElementToken,
    pub(crate) session: PageSession,
    pub(crate) tab_id: String,
}

impl RefTarget {
    /// `@s8f3k2p9:e4 button "Save"`, how the result of an action names it.
    pub(crate) fn label(&self) -> String {
        match &self.entry.name {
            Some(name) => format!("{} {} {}", self.ref_id, self.entry.role, quote(name)),
            None => format!("{} {}", self.ref_id, self.entry.role),
        }
    }
}

impl Session {
    /// Looks `text` up in the snapshot it names and re-identifies the
    /// element: its document must be the one the snapshot read, and its role
    /// and accessible name must still be the recorded ones. Anything else is
    /// `STALE_REF`, so a ref never reaches a different element. The ref's tab
    /// becomes the current tab.
    pub(crate) fn resolve_ref(&mut self, text: &str) -> Result<RefTarget, AppError> {
        let ref_id = RefId::parse(text)?;
        let (map, entry) = self.store().entry(&ref_id)?;
        let token = ElementToken::parse(&entry.token)?;
        let session = self.attach(&token.target_id)?;
        let deadline = self.deadline();
        let live = read_node_identity(self.connection(), &session, &token, deadline)?;
        let live_name = live.identity.name.filter(|name| !name.is_empty());
        if live.role != entry.role || live_name != entry.name {
            return Err(AppError::stale_ref_because(format!(
                "{ref_id} changed since its snapshot"
            ))
            .with_details(json!({
                "kind": "cdp_node_drift",
                "captured": { "role": entry.role, "name": entry.name },
                "live": { "role": live.role, "name": live_name },
            })));
        }
        self.use_tab(&map.tab_id)?;
        Ok(RefTarget {
            ref_id,
            entry,
            token,
            session,
            tab_id: map.tab_id,
        })
    }
}

fn quote(text: &str) -> String {
    crate::view::node_line::quote_text(text)
}
