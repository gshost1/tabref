//! The tab the user is looking at, for a command that names no tab and has
//! none remembered: the focused tab (`document.hasFocus()`), else the first
//! visible one (`document.visibilityState`), in the listing's order. Each
//! tab is asked in a session of its own, attached and detached here without
//! claiming the tab, and all tabs are asked at once, so the question costs
//! three round trips whatever the number of tabs. A tab that does not answer
//! within the budget (busy, frozen, or showing a dialog) counts as neither.

use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::{CdpConnection, PageTarget};
use tabref_core::Deadline;

const PROBE_BUDGET: Duration = Duration::from_millis(1_500);
const DETACH_BUDGET: Duration = Duration::from_millis(500);
const ASK: &str = "[document.visibilityState === 'visible', document.hasFocus()]";

/// What a tab said about itself.
pub(crate) struct TabState {
    pub(crate) target_id: String,
    pub(crate) visible: bool,
    pub(crate) focused: bool,
}

/// The visible tab among `tabs`, or `None` when none says it is visible or
/// focused (or the tabs could not be asked).
pub(crate) fn visible_tab(
    connection: &mut CdpConnection,
    tabs: &[PageTarget],
    deadline: Deadline,
) -> Option<String> {
    let budget = deadline.capped(PROBE_BUDGET);
    let attach: Vec<(&str, Value)> = tabs
        .iter()
        .map(|tab| {
            (
                "Target.attachToTarget",
                json!({ "targetId": tab.target_id, "flatten": true }),
            )
        })
        .collect();
    let sessions: Vec<(String, String)> = tabs
        .iter()
        .zip(each(connection, &attach, None, budget))
        .filter_map(|(tab, reply)| {
            let session = reply?["sessionId"].as_str()?.to_owned();
            Some((tab.target_id.clone(), session))
        })
        .collect();
    let states = ask(connection, &sessions, budget);
    detach(connection, &sessions, deadline.capped(DETACH_BUDGET));
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

fn ask(
    connection: &mut CdpConnection,
    sessions: &[(String, String)],
    budget: Deadline,
) -> Vec<TabState> {
    let params = json!({ "expression": ASK, "returnByValue": true });
    let ids: Vec<Option<u64>> = sessions
        .iter()
        .map(|(_, session)| {
            connection
                .send("Runtime.evaluate", params.clone(), Some(session), budget)
                .ok()
        })
        .collect();
    sessions
        .iter()
        .zip(collect(connection, &ids, budget))
        .filter_map(|((target_id, _), reply)| {
            let value = reply?;
            let answer = value["result"]["value"].as_array()?;
            Some(TabState {
                target_id: target_id.clone(),
                visible: answer.first()?.as_bool()?,
                focused: answer.get(1)?.as_bool()?,
            })
        })
        .collect()
}

fn detach(connection: &mut CdpConnection, sessions: &[(String, String)], budget: Deadline) {
    let calls: Vec<(&str, Value)> = sessions
        .iter()
        .map(|(_, session)| ("Target.detachFromTarget", json!({ "sessionId": session })))
        .collect();
    let _ = each(connection, &calls, None, budget);
}

/// Sends every call before waiting for any reply, and returns each reply in
/// order (`None` for one that failed or missed `budget`).
fn each(
    connection: &mut CdpConnection,
    calls: &[(&str, Value)],
    session_id: Option<&str>,
    budget: Deadline,
) -> Vec<Option<Value>> {
    let ids: Vec<Option<u64>> = calls
        .iter()
        .map(|(method, params)| {
            connection
                .send(method, params.clone(), session_id, budget)
                .ok()
        })
        .collect();
    collect(connection, &ids, budget)
}

fn collect(
    connection: &mut CdpConnection,
    ids: &[Option<u64>],
    budget: Deadline,
) -> Vec<Option<Value>> {
    ids.iter()
        .map(|id| id.and_then(|id| connection.reply(id, budget).ok()))
        .collect()
}

#[cfg(test)]
#[path = "visible_tab_tests.rs"]
mod tests;
