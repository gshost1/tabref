//! What a connection remembers about each flattened session: whether the
//! browser detached it, and which idempotent switches it already turned on.
//! A switch that is on answers a repeat request locally, so a long-lived
//! connection pays for `Page.enable` once per session instead of once per
//! command.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

const MAX_DETACHED_SESSIONS: usize = 1024;
const BROWSER: &str = "";

const DOMAINS: &[(&str, &str)] = &[("Page.enable", "Page.disable")];
const TOGGLES: &[(&str, &str)] = &[
    ("Page.setLifecycleEventsEnabled", "enabled"),
    ("Target.setDiscoverTargets", "discover"),
];

#[derive(Debug, Default)]
pub(crate) struct SessionLedger {
    detached: HashSet<String>,
    switched_on: HashMap<String, Vec<String>>,
}

impl SessionLedger {
    pub(crate) fn is_detached(&self, session_id: &str) -> bool {
        self.detached.contains(session_id)
    }

    pub(crate) fn mark_detached(&mut self, session_id: &str) {
        if self.detached.len() >= MAX_DETACHED_SESSIONS {
            self.detached.clear();
        }
        self.detached.insert(session_id.to_owned());
        self.switched_on.remove(session_id);
    }

    /// Whether a reply to `method` can change what the ledger records.
    pub(crate) fn tracks(method: &str) -> bool {
        DOMAINS
            .iter()
            .any(|(enable, disable)| *enable == method || *disable == method)
            || TOGGLES.iter().any(|(name, _)| *name == method)
    }

    /// Whether `method` with `params` only repeats a switch this session
    /// already turned on, so the browser need not be asked.
    pub(crate) fn already_on(
        &self,
        method: &str,
        params: &Value,
        session_id: Option<&str>,
    ) -> bool {
        let Some(switch) = switch_on(method, params) else {
            return false;
        };
        self.switched_on
            .get(session_id.unwrap_or(BROWSER))
            .is_some_and(|on| on.iter().any(|name| name == switch))
    }

    /// Records the effect of a request the browser accepted.
    pub(crate) fn record(&mut self, method: &str, params: &Value, session_id: Option<&str>) {
        let session = session_id.unwrap_or(BROWSER);
        if let Some(switch) = switch_on(method, params) {
            let on = self.switched_on.entry(session.to_owned()).or_default();
            if !on.iter().any(|name| name == switch) {
                on.push(switch.to_owned());
            }
            return;
        }
        if let Some(switch) = switch_off(method, params) {
            if let Some(on) = self.switched_on.get_mut(session) {
                on.retain(|name| name != switch);
            }
        }
    }
}

fn switch_on<'a>(method: &'a str, params: &Value) -> Option<&'a str> {
    if DOMAINS.iter().any(|(enable, _)| *enable == method) && is_empty(params) {
        return Some(method);
    }
    TOGGLES
        .iter()
        .find(|(name, flag)| *name == method && params.get(flag) == Some(&Value::Bool(true)))
        .filter(|(_, _)| params.as_object().is_some_and(|object| object.len() == 1))
        .map(|_| method)
}

fn switch_off<'a>(method: &str, params: &Value) -> Option<&'a str> {
    if let Some((enable, _)) = DOMAINS.iter().find(|(_, disable)| *disable == method) {
        return Some(enable);
    }
    TOGGLES
        .iter()
        .find(|(name, _)| *name == method)
        .filter(|(_, flag)| params.get(flag) != Some(&Value::Bool(true)))
        .map(|(name, _)| *name)
}

fn is_empty(params: &Value) -> bool {
    params.is_null() || params.as_object().is_some_and(serde_json::Map::is_empty)
}

#[cfg(test)]
#[path = "session_ledger_tests.rs"]
mod tests;
