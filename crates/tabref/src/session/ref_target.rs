use std::collections::HashMap;

use serde_json::json;
use tabref_cdp::PageSession;
use tabref_core::{AccessibilityNode, AppError, ErrorCode, RefEntry, RefId, RefMap, is_ref_able};
use tabref_page::{ElementToken, read_node_identity};

use super::Session;
use crate::view::node_line::node_line;

const NEXT_NUMBER: &str = "refs-next";
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
    let digits = trimmed
        .strip_prefix('@')
        .unwrap_or(trimmed)
        .strip_prefix('e')?;
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
    /// different element. A short ref `e4` is element 4 of the current tab's
    /// latest snapshot. Numbers are handed out once per session across all
    /// tabs and stay with their element, so a short ref the agent holds names
    /// what it named, is `STALE_REF` once that element is gone, and is
    /// `STALE_REF` naming the other tab when it belongs to one. A long ref's
    /// tab becomes the current tab.
    pub(crate) fn resolve_ref(&mut self, text: &str) -> Result<RefTarget, AppError> {
        let ref_id = match short_element(text) {
            Some(element) => self.expand_short_ref(element)?,
            None => RefId::parse(text)?,
        };
        let (map, entry) = self.store().entry(&ref_id)?;
        if entry.token.is_empty() {
            return Err(self.in_another_tab(ref_id.element));
        }
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
        let tab = self.tab_id()?;
        let latest = self
            .store()
            .read_state(&latest_name(&tab))
            .map(|id| id.trim().to_owned())
            .ok_or_else(|| {
                AppError::invalid_args(format!(
                    "e{element} needs a snapshot of this tab; run 'tabref snap' first"
                ))
            })?;
        Ok(RefId::new(latest, element))
    }

    /// The refs a new snapshot of `tab_id` starts from: the tab's latest
    /// snapshot's, so every element keeps its number.
    pub(crate) fn continued_refs(&self, tab_id: &str, url: &str) -> RefMap {
        let mut map = self
            .store()
            .read_state(&latest_name(tab_id))
            .and_then(|id| self.store().load(id.trim()).ok())
            .filter(|map| map.tab_id == tab_id)
            .unwrap_or_else(|| RefMap::new(tab_id, url));
        map.url = url.to_owned();
        map
    }

    /// Numbers the refs of a new snapshot from the session's counter, which
    /// only ever grows. Returns how many nodes got a ref.
    pub(crate) fn number_refs(
        &self,
        tree: &mut AccessibilityNode,
        snapshot_id: &str,
        map: &mut RefMap,
    ) -> Result<usize, AppError> {
        let mut next = self
            .store()
            .read_state(NEXT_NUMBER)
            .and_then(|text| text.trim().parse::<u32>().ok())
            .unwrap_or(1);
        let given = allocate_stable_refs(tree, snapshot_id, map, &mut next);
        self.store().write_state(NEXT_NUMBER, &next.to_string())?;
        Ok(given)
    }

    /// Records `snapshot_id` as the tab's latest snapshot, the one short
    /// refs name.
    pub(crate) fn remember_latest(&self, tab_id: &str, snapshot_id: &str) -> Result<(), AppError> {
        self.store().write_state(&latest_name(tab_id), snapshot_id)
    }

    /// `STALE_REF` for a number this tab's snapshots never gave out, naming
    /// the open tab whose latest snapshot holds it.
    fn in_another_tab(&mut self, element: u32) -> AppError {
        let tabs = self.tabs().unwrap_or_default();
        let owner = tabs.iter().enumerate().find(|(_, tab)| {
            self.store()
                .read_state(&latest_name(&tab.target_id))
                .and_then(|id| self.store().load(id.trim()).ok())
                .and_then(|map| map.get(element).map(|entry| !entry.token.is_empty()))
                .unwrap_or(false)
        });
        let message = match owner {
            Some((index, tab)) => format!(
                "e{element} belongs to tab [{index}] {}, not the current tab; switch with 'tabref tab {index}'",
                quote(&tab.title)
            ),
            None => {
                format!("e{element} belongs to no open tab's latest snapshot; run 'tabref snap'")
            }
        };
        AppError::stale_ref_because(message).with_details(json!({ "kind": "ref_other_tab" }))
    }

    /// `error` with the elements of a fresh snapshot that carry the stale
    /// ref's role and name, so one more step can use the fresh ref.
    fn with_fresh_candidate(
        &mut self,
        mut error: AppError,
        ref_id: &RefId,
        entry: &RefEntry,
    ) -> AppError {
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
                many.iter()
                    .take(FRESH_SHOWN)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
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

/// Gives every ref-able node with an element token a ref in `map`, which
/// holds the tab's earlier refs: a node already numbered keeps its number,
/// and a new node takes `next`, the session's counter, which then moves on,
/// so a number never names two elements in one session. Numbers other tabs
/// took are blank entries in this tab's map. Returns how many nodes got a
/// ref.
pub(crate) fn allocate_stable_refs(
    tree: &mut AccessibilityNode,
    snapshot_id: &str,
    map: &mut RefMap,
    next: &mut u32,
) -> usize {
    let mut numbers: HashMap<String, u32> = map
        .refs
        .iter()
        .enumerate()
        .filter(|(_, entry)| !entry.token.is_empty())
        .map(|(index, entry)| {
            (
                entry.token.clone(),
                u32::try_from(index + 1).unwrap_or(u32::MAX),
            )
        })
        .collect();
    *next = (*next).max(u32::try_from(map.len() + 1).unwrap_or(u32::MAX));
    let mut given = 0;
    let mut pending: Vec<&mut AccessibilityNode> = vec![tree];
    while let Some(node) = pending.pop() {
        if is_ref_able(node)
            && let Some(token) = node.identity.retained_object.clone()
        {
            let entry = RefEntry {
                role: node.role.clone(),
                name: node.identity_name().map(str::to_owned),
                token: token.clone(),
            };
            let element = match numbers.get(&token) {
                Some(&element) => {
                    if let Some(slot) = usize::try_from(element - 1)
                        .ok()
                        .and_then(|index| map.refs.get_mut(index))
                    {
                        *slot = entry;
                    }
                    element
                }
                None => {
                    while map.len() + 1 < usize::try_from(*next).unwrap_or(usize::MAX) {
                        map.push(RefEntry {
                            role: String::new(),
                            name: None,
                            token: String::new(),
                        });
                    }
                    let element = map.push(entry);
                    *next = element.saturating_add(1);
                    numbers.insert(token, element);
                    element
                }
            };
            node.ref_id = Some(RefId::new(snapshot_id, element).to_string());
            given += 1;
        }
        pending.extend(node.children.iter_mut().rev());
    }
    given
}

fn latest_name(tab_id: &str) -> String {
    let safe: String = tab_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("refs-latest-{safe}")
}

fn quote(text: &str) -> String {
    crate::view::node_line::quote_text(text)
}

#[cfg(test)]
#[path = "ref_target_tests.rs"]
mod tests;
