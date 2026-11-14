use std::time::SystemTime;

use serde_json::json;
use tabref_cdp::PageSession;
use tabref_core::{
    AccessibilityNode, AppError, DeliverySemantics, ErrorCode, RefEntry, RefId, RefMap,
};
use tabref_page::{ElementToken, read_node_identity};

use super::Session;
use crate::view::node_line::node_line;

const RECENT_VIEWS: usize = 6;
const FRESH_SHOWN: usize = 3;

/// A ref resolved to a live element: the ref, what its snapshot recorded,
/// the element token, the session that addresses the element (its tab's, or
/// an out-of-process iframe's), the tab it belongs to, and the element
/// tokens of the snapshot it came from, which an action compares against to
/// name the controls it made appear.
#[derive(Debug, Clone)]
pub(crate) struct RefTarget {
    pub(crate) ref_id: RefId,
    pub(crate) entry: RefEntry,
    pub(crate) token: ElementToken,
    pub(crate) session: PageSession,
    pub(crate) tab_id: String,
    pub(crate) known: Vec<String>,
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

/// Whether `text` is a short ref, `e4` or `@e4`, which names element 4 of
/// the current tab's latest snapshot.
pub(crate) fn is_short_ref(text: &str) -> bool {
    short_element(text).is_some()
}

fn short_element(text: &str) -> Option<u32> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix('@').unwrap_or(trimmed).strip_prefix('e')?;
    if digits.is_empty() || digits.len() > 9 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().filter(|number| *number > 0)
}

impl Session {
    /// Looks `text` up in the snapshot it names and re-identifies the
    /// element: its document must be the one the snapshot read, and its role
    /// and accessible name must still be the recorded ones. Anything else is
    /// `STALE_REF` naming the fresh candidate, so a ref never reaches a
    /// different element. A short ref `e4` is looked up in the current tab's
    /// latest snapshot, and only when every recent view of the same address
    /// gave `e4` to the same element. The ref's tab becomes the current tab.
    pub(crate) fn resolve_ref(&mut self, text: &str) -> Result<RefTarget, AppError> {
        let ref_id = match short_element(text) {
            Some(element) => self.expand_short_ref(element)?,
            None => RefId::parse(text)?,
        };
        let (map, entry) = self.store().entry(&ref_id)?;
        let token = ElementToken::parse(&entry.token)?;
        let session = self.attach(&token.target_id)?;
        let deadline = self.deadline();
        let live = match read_node_identity(self.connection(), &session, &token, deadline) {
            Ok(live) => live,
            Err(error) if error.code == ErrorCode::StaleRef => {
                return Err(self.with_fresh_candidate(error, &ref_id, &entry));
            }
            Err(error) => return Err(error),
        };
        let live_name = live.identity.name.filter(|name| !name.is_empty());
        if live.role != entry.role || live_name != entry.name {
            let drift = AppError::stale_ref_because(format!("{ref_id} changed since its snapshot"))
                .with_details(json!({
                    "kind": "cdp_node_drift",
                    "captured": { "role": entry.role, "name": entry.name },
                    "live": { "role": live.role, "name": live_name },
                }));
            return Err(self.with_fresh_candidate(drift, &ref_id, &entry));
        }
        self.use_tab(&map.tab_id)?;
        Ok(RefTarget {
            ref_id,
            entry,
            token,
            session,
            tab_id: map.tab_id,
            known: map.refs.into_iter().map(|entry| entry.token).collect(),
        })
    }

    fn expand_short_ref(&mut self, element: u32) -> Result<RefId, AppError> {
        let latest = self.store().latest_id().ok_or_else(|| {
            AppError::invalid_args(format!("e{element} needs a snapshot; run 'tabref snap' first"))
        })?;
        let map = self.store().load(&latest)?;
        let tab = self.tab_id()?;
        let short = format!("e{element}");
        if map.tab_id != tab {
            return Err(AppError::stale_ref_because(format!(
                "{short} names the latest snapshot, which shows another tab; use the full ref @{latest}:{short}"
            )));
        }
        let entry = map
            .get(element)
            .ok_or_else(|| AppError::stale_ref(&format!("@{latest}:{short}")))?;
        for (id, other) in recent_maps(self, &latest) {
            let differs = other.tab_id == map.tab_id
                && other.url == map.url
                && other.get(element).is_some_and(|seen| seen.token != entry.token);
            if differs {
                return Err(AppError::new(
                    ErrorCode::AmbiguousTarget,
                    format!(
                        "{short} named other elements in earlier views of this page; use the full ref: @{latest}:{short} (latest) or @{id}:{short}"
                    ),
                )
                .with_details(json!({ "kind": "short_ref_ambiguous" }))
                .with_disposition(DeliverySemantics::not_delivered()));
            }
        }
        Ok(RefId::new(latest, element))
    }

    /// `error` with the elements of a fresh snapshot that carry the stale
    /// ref's role and name, so one more step can use the fresh ref.
    fn with_fresh_candidate(&mut self, mut error: AppError, ref_id: &RefId, entry: &RefEntry) -> AppError {
        let Ok(captured) = self.capture() else {
            return error;
        };
        let mut fresh = Vec::new();
        same_identity(&captured.tree, entry, &mut fresh);
        error.message = match fresh.as_slice() {
            [] => format!(
                "{}; the page no longer shows {} {}",
                error.message,
                entry.role,
                entry.name.as_deref().map(quote).unwrap_or_default()
            ),
            [only] => format!("{}; fresh: {only} (retry with it)", error.message),
            many => format!(
                "{}; fresh candidates: {}",
                error.message,
                many.iter().take(FRESH_SHOWN).cloned().collect::<Vec<_>>().join("; ")
            ),
        };
        error.suggestion = None;
        if let Some(details) = error.details.as_mut() {
            details["fresh"] = json!(fresh);
            details["stale_ref"] = json!(ref_id.to_string());
        }
        error
    }
}

fn same_identity(node: &AccessibilityNode, entry: &RefEntry, found: &mut Vec<String>) {
    if node.ref_id.is_some()
        && node.role == entry.role
        && node.identity_name().map(str::to_owned) == entry.name
    {
        found.push(node_line(node, 80));
    }
    for child in &node.children {
        same_identity(child, entry, found);
    }
}

fn recent_maps(session: &Session, latest: &str) -> Vec<(String, RefMap)> {
    let Ok(entries) = std::fs::read_dir(session.store().base_dir().join("snapshots")) else {
        return Vec::new();
    };
    let mut dated: Vec<(SystemTime, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let modified = entry.metadata().and_then(|meta| meta.modified()).ok()?;
            let name = entry.file_name().into_string().ok()?;
            (name != latest).then_some((modified, name))
        })
        .collect();
    dated.sort_by(|a, b| b.0.cmp(&a.0));
    dated
        .into_iter()
        .take(RECENT_VIEWS)
        .filter_map(|(_, id)| {
            let map = session.store().load(&id).ok()?;
            Some((id, map))
        })
        .collect()
}

fn quote(text: &str) -> String {
    crate::view::node_line::quote_text(text)
}
