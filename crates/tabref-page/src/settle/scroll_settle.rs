use std::time::{Duration, Instant};

use serde_json::json;
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline};

use super::scroll_outcome::ScrollOutcome;
use super::settle_tracker::SettleTracker;
use super::settle_wait::SettleWait;
use crate::page::page_scroll::{
    ScrollCount, count_added, read_loaded, scroll_step, wait_frames, watch_scroll,
};
use crate::page::scroll_site::ScrollSite;
use crate::{CdpConnection, CdpEvent, PageSession};

const SETTLE_CAP: Duration = Duration::from_millis(2_000);
const FRAMES_CAP: Duration = Duration::from_millis(300);
const QUIET: Duration = Duration::from_millis(50);
const NETWORK_CAP: Duration = Duration::from_millis(1_500);
const AFTER_NETWORK: Duration = Duration::from_millis(100);
const DOM_QUIET: Duration = Duration::from_millis(30);
const DOM_QUIET_CAP: Duration = Duration::from_millis(300);
const END_GRACE: Duration = Duration::from_millis(500);
const ACTIVE_END_GRACE: Duration = Duration::from_millis(800);
const LOADER_CAP: Duration = Duration::from_millis(1_500);
const POLL: Duration = Duration::from_millis(40);
const CLEANUP: Duration = Duration::from_millis(500);
const CLEANUP_MS: u64 = 500;

/// Scrolls `site` by `(dx, dy)` CSS pixels. A scroll that ends far from
/// the end of the scrolled content, or cannot move, is one page script and
/// returns at once. One that ends within one and a half views of the end
/// turns network events on first, then scrolls and waits for what it set
/// loading: one frame and two tasks for `IntersectionObserver` callbacks
/// and scroll events; XHR and fetch requests from any origin until
/// they finish and 50 ms pass without another (until 1.5 s after the
/// frame), with 100 ms after each for their rows to land; inserted
/// elements without text yet (a spinner) up to 1.5 s for content to come;
/// at the very end 500 ms for a late loader, extended to 800 ms if it
/// starts a request; then 30 ms without new elements (at most
/// 300 ms). All of it stops 2 s after the scroll, so a page that polls or
/// animates costs at most that, and a DOM that only changes in place (a
/// clock) extends none of the waits. A tab that draws no frames ran none
/// of its loaders and is not waited for.
pub fn scroll_and_settle(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    delta: (f64, f64),
    deadline: Deadline,
) -> Result<ScrollOutcome, AppError> {
    let step = scroll_step(connection, session, site, delta, deadline)?;
    if !step.near {
        return Ok(ScrollOutcome {
            moved: step.moved,
            at_end: step.at_end,
            hidden: step.hidden,
            loading: step.at_end && step.pending,
            ..ScrollOutcome::default()
        });
    }
    let cap = deadline.capped(SETTLE_CAP);
    let id = session.session_id.clone();
    let _ = connection.call("Network.enable", json!({}), Some(&id), cap);
    connection.discard_events(|event| is_network_event(event, &id));
    let outcome = watch(connection, session, site, (cap, deadline));
    let cleanup = Deadline::after(CLEANUP_MS).unwrap_or(deadline);
    let _ = connection.call("Network.disable", json!({}), Some(&id), cleanup);
    connection.discard_events(|event| is_network_event(event, &id));
    outcome
}

fn watch(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    (cap, deadline): (Deadline, Deadline),
) -> Result<ScrollOutcome, AppError> {
    let Some((moved, at_end)) = watch_scroll(connection, session, site, cap)? else {
        return Ok(ScrollOutcome::default());
    };
    let mut outcome = ScrollOutcome {
        moved,
        watched: true,
        ..ScrollOutcome::default()
    };
    let mut tracker = SettleTracker::for_scroll();
    let framed = match wait_frames(connection, session, site, cap.capped(FRAMES_CAP)) {
        Ok(framed) => framed,
        Err(error) if is_timeout(&error) => false,
        Err(_) => return Ok(unwatched(outcome)),
    };
    if framed {
        match settle(connection, session, site, (&mut tracker, at_end), cap) {
            Err(error) if !is_timeout(&error) => return Ok(unwatched(outcome)),
            _ => {}
        }
    }
    let read = read_loaded(connection, session, site, deadline.capped(CLEANUP));
    let Ok(Some(loaded)) = read else {
        return Ok(unwatched(outcome));
    };
    let requests = matches!(
        tracker.next_wait(Instant::now(), QUIET),
        SettleWait::Network
    );
    let changing = loaded.mutations > 0 && loaded.items == 0 && !loaded.grew;
    outcome.loading = requests || loaded.pending || (loaded.at_end && changing);
    outcome.hidden = !framed;
    outcome.at_end = loaded.at_end;
    outcome.items = loaded.items;
    outcome.first = loaded.first;
    outcome.last = loaded.last;
    outcome.lines = loaded.lines;
    outcome.more_lines = loaded.more_lines;
    Ok(outcome)
}

fn unwatched(outcome: ScrollOutcome) -> ScrollOutcome {
    ScrollOutcome {
        unwatched: true,
        ..outcome
    }
}

fn settle(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    (tracker, at_end): (&mut SettleTracker, bool),
    cap: Deadline,
) -> Result<(), AppError> {
    let id = session.session_id.clone();
    let started = Instant::now();
    let mut landed = None::<Instant>;
    absorb(connection, &id, tracker);
    loop {
        if cap.is_expired() {
            return Ok(());
        }
        if follow_network(connection, &id, tracker, (started, cap))? {
            landed = Some(Instant::now());
        }
        let Some(count) = count_added(connection, session, site, cap)? else {
            return Ok(());
        };
        if count.items > 0 || count.grew {
            return dom_quiet(connection, session, site, cap);
        }
        let waited = started.elapsed();
        let grace = if at_end && tracker.saw_network() {
            ACTIVE_END_GRACE
        } else if at_end {
            END_GRACE
        } else {
            Duration::ZERO
        };
        let expecting = (count.inserted > 0 && waited < LOADER_CAP)
            || waited < grace
            || landed.is_some_and(|at| at.elapsed() < AFTER_NETWORK);
        if !expecting {
            return Ok(());
        }
        if let Some(event) = next(connection, &id, cap.capped(POLL))? {
            tracker.absorb(&event, Instant::now());
        }
        absorb(connection, &id, tracker);
    }
}

/// Follows the requests seen so far until they finish and 50 ms pass
/// without another, or 1.5 s after `started`. Whether there were any.
fn follow_network(
    connection: &mut CdpConnection,
    session_id: &str,
    tracker: &mut SettleTracker,
    (started, cap): (Instant, Deadline),
) -> Result<bool, AppError> {
    let mut followed = false;
    loop {
        let now = Instant::now();
        let wait = match tracker.next_wait(now, QUIET) {
            SettleWait::Done | SettleWait::Navigation => return Ok(followed),
            SettleWait::Network => {
                let Some(left) = NETWORK_CAP.checked_sub(now - started) else {
                    return Ok(followed);
                };
                cap.capped(left)
            }
            SettleWait::QuietUntil(until) => cap.capped(until - now),
        };
        if cap.is_expired() {
            return Ok(followed);
        }
        followed = true;
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
    let growth = |count: Option<ScrollCount>| count.map(|count| (count.inserted, count.items));
    let mut seen = growth(count_added(connection, session, site, limit)?);
    while !limit.is_expired() {
        let _ = next(connection, &session.session_id, limit.capped(DOM_QUIET))?;
        let now = growth(count_added(connection, session, site, limit)?);
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
