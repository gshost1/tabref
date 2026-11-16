use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::dialog_json::{DIALOG_OPENING, drain_dialogs};
use super::page_barrier::page_turn;
use super::settle_outcome::SettleOutcome;
use super::settle_tracker::SettleTracker;
use super::settle_wait::SettleWait;
use crate::act::change_watch::mutation_count;
use crate::{CdpConnection, CdpEvent, PageSession};
use tabref_cdp::{is_timeout, list_frames, transport_failed};

const SETTLE_CAP: Duration = Duration::from_millis(5_000);
const TURN_CAP: Duration = Duration::from_millis(1_000);
const QUIET: Duration = Duration::from_millis(50);
const NETWORK_CAP: Duration = Duration::from_millis(1_500);
const DOM_QUIET: Duration = Duration::from_millis(30);
const DOM_QUIET_CAP: Duration = Duration::from_millis(300);

/// Turns on the page and network events of `session` before an action and
/// drops any already buffered, so [`settle_action`] sees only what the
/// action set off. Returns the id of the tab's main frame.
pub fn watch_navigation(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<String, AppError> {
    let session_id = Some(session.session_id.as_str());
    connection.call("Page.enable", json!({}), session_id, deadline)?;
    connection.call("Network.enable", json!({}), session_id, deadline)?;
    let main = list_frames(connection, session, deadline)?
        .into_iter()
        .next()
        .ok_or_else(|| transport_failed("Page.getFrameTree", "the page has no main frame"))?;
    let id = session.session_id.clone();
    connection.discard_events(|event| is_page_event(event, &id));
    Ok(main.frame_id)
}

/// Waits until what an action set off can be judged, from events alone:
/// one turn of the page's event loop shows whether a navigation or request
/// began, and nothing more is waited for when none did. A navigation is
/// followed until it commits and reaches `DOMContentLoaded`; document, XHR
/// and fetch requests until they finish and 50 ms pass without another
/// (at most 1.5 s); and after requests or a route change within the
/// document, until 30 ms pass without a DOM mutation (at most 300 ms). All
/// of it within 5 s. Dialogs that opened meanwhile are reported.
pub fn settle_action(
    connection: &mut CdpConnection,
    session: &PageSession,
    main_frame: &str,
    deadline: Deadline,
) -> Result<SettleOutcome, AppError> {
    let cap = deadline.capped(SETTLE_CAP);
    let id = session.session_id.clone();
    let mut tracker = SettleTracker::new(main_frame);
    match page_turn(connection, session, cap.capped(TURN_CAP)) {
        Err(error) if !is_timeout(&error) => tracker.expect_navigation(),
        _ => {}
    }
    for event in connection.drain_events(|event| is_page_event(event, &id)) {
        tracker.absorb(&event, Instant::now());
    }
    follow(connection, &id, &mut tracker, cap)?;
    if !tracker.committed() && (tracker.same_document() || tracker.saw_network()) {
        dom_quiet(connection, session, &mut tracker, cap)?;
    }
    let dialogs = drain_dialogs(connection);
    connection.discard_events(|event| is_page_event(event, &id));
    let _ = connection.call("Network.disable", json!({}), Some(&id), cap);
    Ok(SettleOutcome {
        navigated: tracker.committed(),
        same_document: tracker.same_document(),
        network: tracker.saw_network(),
        dialogs,
    })
}

/// Waits up to `window` for a navigation of the main frame that an action
/// started late, after it had settled (a handler that navigates from a
/// timer), and follows it as [`settle_action`] would. Returns whether the
/// tab committed a new document. Page events stay on after an action, so a
/// navigation that began in between is already buffered and costs nothing
/// to see.
pub fn follow_late_navigation(
    connection: &mut CdpConnection,
    session: &PageSession,
    main_frame: &str,
    window: Duration,
    deadline: Deadline,
) -> Result<bool, AppError> {
    let cap = deadline.capped(SETTLE_CAP);
    let id = session.session_id.clone();
    let mut tracker = SettleTracker::new(main_frame);
    let started = |event: &CdpEvent| {
        is_page_event(event, &id)
            && matches!(
                event.method.as_str(),
                "Page.frameRequestedNavigation"
                    | "Page.frameScheduledNavigation"
                    | "Page.frameStartedNavigating"
                    | "Page.frameStartedLoading"
            )
            && event.params.get("frameId").and_then(Value::as_str) == Some(main_frame)
    };
    let first = match connection.wait_for_event(started, cap.capped(window)) {
        Ok(event) => event,
        Err(error) if is_timeout(&error) => return Ok(false),
        Err(error) => return Err(error),
    };
    tracker.absorb(&first, Instant::now());
    if !tracker.navigating() {
        return Ok(false);
    }
    for event in connection.drain_events(|event| is_page_event(event, &id)) {
        tracker.absorb(&event, Instant::now());
    }
    follow(connection, &id, &mut tracker, cap)?;
    connection.discard_events(|event| is_page_event(event, &id));
    Ok(tracker.committed())
}

fn follow(
    connection: &mut CdpConnection,
    session_id: &str,
    tracker: &mut SettleTracker,
    cap: Deadline,
) -> Result<(), AppError> {
    let mut network_since: Option<Instant> = None;
    loop {
        let now = Instant::now();
        let wait = match tracker.next_wait(now, QUIET) {
            SettleWait::Done => return Ok(()),
            SettleWait::Navigation => cap,
            SettleWait::Network => {
                let since = *network_since.get_or_insert(now);
                let Some(left) = NETWORK_CAP.checked_sub(now - since) else {
                    return Ok(());
                };
                cap.capped(left)
            }
            SettleWait::QuietUntil(until) => cap.capped(until - now),
        };
        if cap.is_expired() {
            return Ok(());
        }
        if let Some(event) = next(connection, session_id, wait)? {
            tracker.absorb(&event, Instant::now());
        }
    }
}

fn dom_quiet(
    connection: &mut CdpConnection,
    session: &PageSession,
    tracker: &mut SettleTracker,
    cap: Deadline,
) -> Result<(), AppError> {
    let limit = cap.capped(DOM_QUIET_CAP);
    let mut seen = mutation_count(connection, session, limit)?;
    while !limit.is_expired() {
        let window = limit.capped(DOM_QUIET);
        while let Some(event) = next(connection, &session.session_id, window)? {
            tracker.absorb(&event, Instant::now());
        }
        let now = mutation_count(connection, session, limit)?;
        if now == seen {
            return Ok(());
        }
        seen = now;
    }
    Ok(())
}

fn next(
    connection: &mut CdpConnection,
    session_id: &str,
    deadline: Deadline,
) -> Result<Option<CdpEvent>, AppError> {
    match connection.wait_for_event(|event| is_page_event(event, session_id), deadline) {
        Ok(event) => Ok(Some(event)),
        Err(error) if is_timeout(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn is_page_event(event: &CdpEvent, session_id: &str) -> bool {
    event.session_id.as_deref() == Some(session_id)
        && event.method != DIALOG_OPENING
        && (event.method.starts_with("Page.") || event.method.starts_with("Network."))
}

#[cfg(test)]
#[path = "action_settle_tests.rs"]
mod tests;
