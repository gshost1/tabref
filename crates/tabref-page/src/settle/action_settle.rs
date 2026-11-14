use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::settle_outcome::SettleOutcome;
use crate::{CdpConnection, CdpEvent, PageSession};
use tabref_cdp::is_timeout;

const START_WINDOW: Duration = Duration::from_millis(60);
const QUIET: Duration = Duration::from_millis(100);
const SETTLE_CAP: Duration = Duration::from_millis(3_000);
const DIALOG_OPENING: &str = "Page.javascriptDialogOpening";

const NAVIGATION_STARTS: &[&str] = &[
    "Page.frameScheduledNavigation",
    "Page.frameRequestedNavigation",
    "Page.frameStartedNavigating",
    "Page.frameStartedLoading",
];
const NAVIGATION_ENDS: &[&str] = &[
    "Page.frameStoppedLoading",
    "Page.loadEventFired",
    "Page.navigatedWithinDocument",
];

/// Turns on the page's navigation events before an action and drops any
/// already buffered, so [`settle_action`] sees only what the action set off.
pub fn watch_navigation(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<(), AppError> {
    connection.call(
        "Page.enable",
        json!({}),
        Some(&session.session_id),
        deadline,
    )?;
    connection.discard_events(|event| is_navigation(event) || event.method == DIALOG_OPENING);
    Ok(())
}

/// Waits for what an action set off to settle, after Chrome DevTools MCP's
/// pattern: up to 60 ms for a navigation to start, returning at once when
/// none does; once one starts, until the page stops loading, then until
/// 100 ms pass with no navigation event, all within 3 s. Dialogs that opened
/// meanwhile are reported.
pub fn settle_action(
    connection: &mut CdpConnection,
    deadline: Deadline,
) -> Result<SettleOutcome, AppError> {
    let cap = deadline.capped(SETTLE_CAP);
    let mut outcome = SettleOutcome::default();
    if next(connection, is_start, cap.capped(START_WINDOW))?.is_some() {
        outcome.navigated = true;
        let ended = |event: &CdpEvent| NAVIGATION_ENDS.contains(&event.method.as_str());
        if next(connection, ended, cap)?.is_some() {
            while next(connection, is_navigation, cap.capped(QUIET))?.is_some() {}
        }
    }
    outcome.dialogs = connection
        .drain_events(|event| event.method == DIALOG_OPENING)
        .into_iter()
        .map(|event| {
            json!({
                "type": event.params.get("type").cloned().unwrap_or(Value::Null),
                "message": event.params.get("message").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    connection.discard_events(is_navigation);
    Ok(outcome)
}

fn next(
    connection: &mut CdpConnection,
    matches: impl FnMut(&CdpEvent) -> bool,
    deadline: Deadline,
) -> Result<Option<CdpEvent>, AppError> {
    match connection.wait_for_event(matches, deadline) {
        Ok(event) => Ok(Some(event)),
        Err(error) if is_timeout(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn is_start(event: &CdpEvent) -> bool {
    event.session_id.is_some() && NAVIGATION_STARTS.contains(&event.method.as_str())
}

fn is_navigation(event: &CdpEvent) -> bool {
    event.session_id.is_some()
        && (NAVIGATION_STARTS.contains(&event.method.as_str())
            || NAVIGATION_ENDS.contains(&event.method.as_str()))
}

#[cfg(test)]
#[path = "action_settle_tests.rs"]
mod tests;
