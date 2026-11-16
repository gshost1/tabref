use std::time::{Duration, Instant};

use serde_json::json;
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline};

use super::scroll_outcome::ScrollOutcome;
use super::settle_tracker::SettleTracker;
use super::settle_wait::SettleWait;
use crate::page::page_scroll::{
    count_added, listens_for_scroll, read_loaded, scroll_step, wait_frames,
};
use crate::page::scroll_site::ScrollSite;
use crate::{CdpConnection, CdpEvent, PageSession};

const SETTLE_CAP: Duration = Duration::from_millis(5_000);
const FRAMES_CAP: Duration = Duration::from_millis(300);
const QUIET: Duration = Duration::from_millis(50);
const NETWORK_CAP: Duration = Duration::from_millis(1_500);
const DOM_QUIET: Duration = Duration::from_millis(30);
const DOM_QUIET_CAP: Duration = Duration::from_millis(300);
const LISTENER_GRACE: Duration = Duration::from_millis(250);
const LOADER_CAP: Duration = Duration::from_millis(1_500);
const POLL: Duration = Duration::from_millis(40);

/// Scrolls `site` by `(dx, dy)` CSS pixels and, when that brings the end of
/// the scrolled content within reach, waits for what the scroll set
/// loading, under the rules a click settles by: same-site document, XHR and
/// fetch requests until they finish and 50 ms pass without another (at
/// most 1.5 s), then until 30 ms pass without a DOM mutation (at most
/// 300 ms), all within 5 s. Before judging, two animation frames let
/// `IntersectionObserver` callbacks and scroll events run; a page that
/// listens for scroll events gets 250 ms more for a debounced loader, and
/// one whose DOM changed without adding content yet (a spinner) up to
/// 1.5 s for the content to come. A scroll far from the end returns as
/// soon as it moved. Network events are on from just before the scroll,
/// since a sentinel's callback may fire with the next frame.
pub fn scroll_and_settle(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    delta: (f64, f64),
    deadline: Deadline,
) -> Result<ScrollOutcome, AppError> {
    let cap = deadline.capped(SETTLE_CAP);
    let id = session.session_id.clone();
    connection.call("Network.enable", json!({}), Some(&id), cap)?;
    connection.discard_events(|event| is_network_event(event, &id));
    let step = match scroll_step(connection, session, site, delta, deadline) {
        Ok(step) if step.near => step,
        other => {
            let _ = connection.call("Network.disable", json!({}), Some(&id), cap);
            connection.discard_events(|event| is_network_event(event, &id));
            let step = other?;
            return Ok(ScrollOutcome {
                moved: step.moved,
                at_end: step.at_end,
                ..ScrollOutcome::default()
            });
        }
    };
    let mut tracker = SettleTracker::new("");
    let waited = settle(connection, session, site, &mut tracker, cap);
    let _ = connection.call("Network.disable", json!({}), Some(&id), cap);
    connection.discard_events(|event| is_network_event(event, &id));
    waited?;
    let loaded = read_loaded(connection, session, site, cap).unwrap_or_default();
    Ok(ScrollOutcome {
        moved: step.moved,
        watched: true,
        at_end: if loaded.items > 0 {
            loaded.at_end
        } else {
            step.at_end
        },
        items: loaded.items,
        first: loaded.first,
        last: loaded.last,
        lines: loaded.lines,
        more_lines: loaded.more_lines,
        loading: !matches!(tracker.next_wait(Instant::now(), QUIET), SettleWait::Done),
    })
}

fn settle(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    tracker: &mut SettleTracker,
    cap: Deadline,
) -> Result<(), AppError> {
    let id = session.session_id.clone();
    match wait_frames(connection, session, site, cap.capped(FRAMES_CAP)) {
        Err(error) if !is_timeout(&error) => return Ok(()),
        _ => {}
    }
    absorb(connection, &id, tracker);
    let started = Instant::now();
    let grace = if tracker.saw_network() {
        Duration::ZERO
    } else if listens_for_scroll(connection, session, site, cap)? {
        LISTENER_GRACE
    } else {
        Duration::ZERO
    };
    loop {
        if cap.is_expired() {
            return Ok(());
        }
        if tracker.saw_network() {
            follow_network(connection, &id, tracker, cap)?;
            return dom_quiet(connection, session, site, cap);
        }
        let Some((mutations, items)) = count_added(connection, session, site, cap)? else {
            return Ok(());
        };
        if items > 0 {
            return dom_quiet(connection, session, site, cap);
        }
        let waited = started.elapsed();
        let expecting = (mutations > 0 && waited < LOADER_CAP) || waited < grace;
        if !expecting {
            return Ok(());
        }
        if let Some(event) = next(connection, &id, cap.capped(POLL))? {
            tracker.absorb(&event, Instant::now());
        }
        absorb(connection, &id, tracker);
    }
}

fn follow_network(
    connection: &mut CdpConnection,
    session_id: &str,
    tracker: &mut SettleTracker,
    cap: Deadline,
) -> Result<(), AppError> {
    let since = Instant::now();
    loop {
        let now = Instant::now();
        let wait = match tracker.next_wait(now, QUIET) {
            SettleWait::Done | SettleWait::Navigation => return Ok(()),
            SettleWait::Network => {
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
    site: ScrollSite<'_>,
    cap: Deadline,
) -> Result<(), AppError> {
    let limit = cap.capped(DOM_QUIET_CAP);
    let mut seen = count_added(connection, session, site, limit)?;
    while !limit.is_expired() {
        let _ = next(connection, &session.session_id, limit.capped(DOM_QUIET))?;
        let now = count_added(connection, session, site, limit)?;
        if now == seen {
            return Ok(());
        }
        seen = now;
    }
    Ok(())
}

fn absorb(connection: &mut CdpConnection, session_id: &str, tracker: &mut SettleTracker) {
    for event in connection.drain_events(|event| is_network_event(event, session_id)) {
        tracker.absorb(&event, Instant::now());
    }
}

fn next(
    connection: &mut CdpConnection,
    session_id: &str,
    deadline: Deadline,
) -> Result<Option<CdpEvent>, AppError> {
    match connection.wait_for_event(|event| is_network_event(event, session_id), deadline) {
        Ok(event) => Ok(Some(event)),
        Err(error) if is_timeout(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn is_network_event(event: &CdpEvent, session_id: &str) -> bool {
    event.session_id.as_deref() == Some(session_id) && event.method.starts_with("Network.")
}
