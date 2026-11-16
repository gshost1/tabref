//! The tab the user is looking at, for a command that names no tab and has
//! none remembered: the focused tab (`document.hasFocus()`), else the first
//! visible one (`document.visibilityState`), in the listing's order. Each
//! tab is asked in a session of its own, attached and detached here without
//! claiming the tab, and all tabs are asked at once.
//!
//! Answers are taken as they come, so a tab that never answers (discarded,
//! hung, or showing a dialog) costs nothing while others do: the wait ends
//! as soon as a tab says it is focused, when every tab has answered, or a
//! moment after the first tab says it is visible, and the rest are given
//! up. The whole probe takes at most 1.5 s and never more than half the
//! time the command has left; the detaches are sent without waiting.
//!
//! In headless Chrome with two windows, a tab opened in a new window keeps
//! `hasFocus()` after another tab is used, so it is the one chosen there.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::json;
use tabref_cdp::{CdpConnection, PageTarget};
use tabref_core::Deadline;

const PROBE_BUDGET: Duration = Duration::from_millis(1_500);
const VISIBLE_GRACE: Duration = Duration::from_millis(100);
const ASK: &str = "[document.visibilityState === 'visible', document.hasFocus()]";

/// What a tab said about itself.
#[derive(Debug, Clone)]
pub(crate) struct TabState {
    pub(crate) target_id: String,
    pub(crate) visible: bool,
    pub(crate) focused: bool,
}

/// The visible tab among `tabs`, or `None` when none says it is visible or
/// focused (or the tabs could not be asked in time).
pub(crate) fn visible_tab(
    connection: &mut CdpConnection,
    tabs: &[PageTarget],
    deadline: Deadline,
) -> Option<String> {
    let budget = deadline.capped(probe_budget(deadline.remaining()));
    let sessions = attach(connection, tabs, budget);
    let states = ask(connection, &sessions, budget);
    for (_, session) in &sessions {
        if let Ok(id) = connection.send(
            "Target.detachFromTarget",
            json!({ "sessionId": session }),
            None,
            budget,
        ) {
            connection.abandon(id);
        }
    }
    pick_visible(&states)
}

/// The focused tab, else the first visible one.
pub(crate) fn pick_visible(states: &[TabState]) -> Option<String> {
    states
        .iter()
        .find(|state| state.focused)
        .or_else(|| states.iter().find(|state| state.visible))
        .map(|state| state.target_id.clone())
}

/// How long the probe may take with `remaining` left for the command.
pub(crate) fn probe_budget(remaining: Duration) -> Duration {
    PROBE_BUDGET.min(remaining / 2)
}

/// Whether the probe has heard enough: a tab said it is focused, all
/// `asked` tabs answered, or `since_visible` (the time since the first tab
/// said it is visible) has run past the grace for a focused answer.
pub(crate) fn settled(
    states: &[TabState],
    answered: usize,
    asked: usize,
    since_visible: Option<Duration>,
) -> bool {
    states.iter().any(|state| state.focused)
        || answered >= asked
        || since_visible.is_some_and(|since| since >= VISIBLE_GRACE)
}

/// A session of its own on each tab that attaches within `budget`, as
/// `(target id, session id)` in the listing's order.
fn attach(
    connection: &mut CdpConnection,
    tabs: &[PageTarget],
    budget: Deadline,
) -> Vec<(String, String)> {
    let sent: Vec<(String, u64)> = tabs
        .iter()
        .filter_map(|tab| {
            let params = json!({ "targetId": tab.target_id, "flatten": true });
            let id = connection
                .send("Target.attachToTarget", params, None, budget)
                .ok()?;
            Some((tab.target_id.clone(), id))
        })
        .collect();
    let mut replies: HashMap<u64, serde_json::Value> = HashMap::new();
    let mut pending: Vec<u64> = sent.iter().map(|(_, id)| *id).collect();
    while !pending.is_empty() {
        let Ok((id, outcome)) = connection.next_reply(&pending, budget) else {
            break;
        };
        pending.retain(|waiting| *waiting != id);
        if let Ok(value) = outcome {
            replies.insert(id, value);
        }
    }
    for id in pending {
        connection.abandon(id);
    }
    sent.into_iter()
        .filter_map(|(target_id, id)| {
            let session = replies.get(&id)?["sessionId"].as_str()?.to_owned();
            Some((target_id, session))
        })
        .collect()
}

/// Each tab's answer, as they come, until [`settled`] or `budget` ends;
/// the tabs still silent then are given up. In the listing's order.
fn ask(
    connection: &mut CdpConnection,
    sessions: &[(String, String)],
    budget: Deadline,
) -> Vec<TabState> {
    let params = json!({ "expression": ASK, "returnByValue": true });
    let sent: Vec<(usize, u64)> = sessions
        .iter()
        .enumerate()
        .filter_map(|(index, (_, session))| {
            let id = connection
                .send("Runtime.evaluate", params.clone(), Some(session), budget)
                .ok()?;
            Some((index, id))
        })
        .collect();
    let mut states: Vec<Option<TabState>> = vec![None; sessions.len()];
    let mut pending: Vec<u64> = sent.iter().map(|(_, id)| *id).collect();
    let mut first_visible: Option<Instant> = None;
    loop {
        let heard: Vec<TabState> = states.iter().flatten().cloned().collect();
        let answered = sent.len() - pending.len();
        let since = first_visible.map(|at| at.elapsed());
        if pending.is_empty() || settled(&heard, answered, sent.len(), since) {
            break;
        }
        let wait = match first_visible {
            Some(at) => budget.capped(VISIBLE_GRACE.saturating_sub(at.elapsed())),
            None => budget,
        };
        let Ok((id, outcome)) = connection.next_reply(&pending, wait) else {
            break;
        };
        pending.retain(|waiting| *waiting != id);
        let Some(index) = sent
            .iter()
            .find(|(_, sent_id)| *sent_id == id)
            .map(|(index, _)| *index)
        else {
            continue;
        };
        let answer = outcome.ok().and_then(|value| {
            let answer = value["result"]["value"].as_array()?.clone();
            Some((answer.first()?.as_bool()?, answer.get(1)?.as_bool()?))
        });
        if let Some((visible, focused)) = answer {
            if visible && first_visible.is_none() {
                first_visible = Some(Instant::now());
            }
            states[index] = Some(TabState {
                target_id: sessions[index].0.clone(),
                visible,
                focused,
            });
        }
    }
    for id in pending {
        connection.abandon(id);
    }
    states.into_iter().flatten().collect()
}

#[cfg(test)]
#[path = "visible_tab_tests.rs"]
mod tests;
