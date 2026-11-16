//! Lets a loaded page finish its own load-time work before it is read: the
//! fetches a load handler sends and what they render. Judged from network
//! events and one quick page read at a time, so a page with nothing left
//! to do costs one read.

use std::time::{Duration, Instant};

use serde_json::Value;
use tabref_core::{AppError, Deadline};

use super::load_watch::{LoadStatus, LoadWatch, is_network_event};
use super::settle_tracker::SettleTracker;
use super::settle_wait::SettleWait;
use crate::{CdpConnection, CdpEvent};
use tabref_cdp::is_timeout;

/// How long after `load` the page's own work is waited for.
pub const LOAD_SETTLE_CAP: Duration = Duration::from_millis(400);
/// How long after `load` a near-empty page that is still fetching or
/// changing is waited for.
pub const RENDER_CAP: Duration = Duration::from_millis(1_500);
const NETWORK_QUIET: Duration = Duration::from_millis(50);
const DOM_QUIET: Duration = Duration::from_millis(50);
const STATUS_SLACK: Duration = Duration::from_millis(100);
const LONGEST_AGE: Duration = Duration::from_secs(10);
const TRACKED: &[&str] = &["XHR", "Fetch"];

/// How the page stood when the wait ended: whether its work settled
/// (no fetch of its own site in flight, 50 ms without one and 50 ms
/// without a DOM change), whether it was near-empty, and how long it took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSettle {
    pub settled: bool,
    pub near_empty: bool,
    pub waited: Duration,
}

impl LoadSettle {
    /// A near-empty page whose work had not settled when the wait gave up.
    pub fn still_rendering(&self) -> bool {
        self.near_empty && !self.settled
    }
}

/// Waits, after `load`, until the page's XHR and fetch requests to its own
/// site have finished and neither they nor the DOM have changed for 50 ms,
/// for at most 400 ms; a page that is near-empty then (see
/// [`LoadStatus`]) gets up to 1.5 s in all, and none of it eats into the
/// last `reserve` of `deadline`. Ends the watch either way. A page that
/// cannot be read, such as one being replaced, ends the wait.
pub fn settle_load(
    connection: &mut CdpConnection,
    mut watch: LoadWatch,
    deadline: Deadline,
    reserve: Duration,
) -> LoadSettle {
    let started = Instant::now();
    watch.forget_script(connection, deadline);
    let bound = deadline.capped(deadline.remaining().saturating_sub(reserve));
    let outcome = follow(connection, &watch, started, bound).unwrap_or(LoadSettle {
        settled: false,
        near_empty: false,
        waited: started.elapsed(),
    });
    watch.stop(connection, deadline);
    outcome
}

fn follow(
    connection: &mut CdpConnection,
    watch: &LoadWatch,
    started: Instant,
    deadline: Deadline,
) -> Result<LoadSettle, AppError> {
    let session_id = watch.session.session_id.clone();
    let mut tracker = SettleTracker::new(&watch.session.target_id);
    let mut cap = LOAD_SETTLE_CAP;
    let read_by = |cap: Duration| {
        deadline.capped((started + cap + STATUS_SLACK).saturating_duration_since(Instant::now()))
    };
    let mut status = watch.status(connection, read_by(cap))?;
    let mut clock = EventClock::new(status.wall_ms, Instant::now());
    for event in connection.drain_events(|event| is_network_event(event, &session_id)) {
        clock.calibrate(&event);
        absorb(&mut tracker, &event, clock.when(&event));
    }
    let mut changed = changed_at(&status, Instant::now());
    loop {
        let now = Instant::now();
        let network = tracker.next_wait(now, NETWORK_QUIET);
        let dom_quiet_at = changed.map(|at| at + DOM_QUIET).filter(|at| *at > now);
        if network == SettleWait::Done && dom_quiet_at.is_none() {
            return Ok(done(true, &status, started));
        }
        if now >= started + cap || deadline.is_expired() {
            if cap < RENDER_CAP && !deadline.is_expired() {
                status = watch.status(connection, read_by(cap))?;
                changed = changed_at(&status, Instant::now());
                if status.near_empty {
                    cap = RENDER_CAP;
                    continue;
                }
            }
            return Ok(done(false, &status, started));
        }
        let until = match network {
            SettleWait::Network | SettleWait::Navigation => started + cap,
            SettleWait::QuietUntil(at) => at,
            SettleWait::Done => dom_quiet_at.unwrap_or(now),
        };
        let until = until.min(started + cap).max(now);
        let window = deadline.capped(until - now);
        match next(connection, &session_id, window)? {
            Some(event) => absorb(&mut tracker, &event, Instant::now()),
            None => {
                status = watch.status(connection, read_by(cap))?;
                changed = changed_at(&status, Instant::now());
            }
        }
    }
}

fn done(settled: bool, status: &LoadStatus, started: Instant) -> LoadSettle {
    LoadSettle {
        settled,
        near_empty: status.near_empty,
        waited: started.elapsed(),
    }
}

fn changed_at(status: &LoadStatus, now: Instant) -> Option<Instant> {
    status
        .since_mutation
        .map(|since| now.checked_sub(since).unwrap_or(now))
}

/// Feeds the tracker the page's XHR and fetch requests only: a document
/// request is the page itself or a frame, which `load` already waited for
/// or an ad slot added later.
fn absorb(tracker: &mut SettleTracker, event: &CdpEvent, at: Instant) {
    if event.method == "Network.requestWillBeSent" {
        let kind = event.params.get("type").and_then(Value::as_str);
        if !kind.is_some_and(|kind| TRACKED.contains(&kind)) {
            return;
        }
    }
    tracker.absorb(event, at);
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

/// When a buffered network event happened, on this process's clock. A
/// request's first event carries both the browser's monotonic time and the
/// wall time; their difference turns any later event's monotonic time into
/// wall time, which the page's own clock (read at `read_at`) turns into an
/// age. Without a calibration an event counts as just now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EventClock {
    page_wall_ms: f64,
    read_at: Instant,
    offset_s: Option<f64>,
}

impl EventClock {
    pub(crate) fn new(page_wall_ms: f64, read_at: Instant) -> Self {
        Self {
            page_wall_ms,
            read_at,
            offset_s: None,
        }
    }

    pub(crate) fn calibrate(&mut self, event: &CdpEvent) {
        if self.offset_s.is_some() {
            return;
        }
        let wall = event.params.get("wallTime").and_then(Value::as_f64);
        let monotonic = event.params.get("timestamp").and_then(Value::as_f64);
        if let (Some(wall), Some(monotonic)) = (wall, monotonic) {
            self.offset_s = Some(wall - monotonic);
        }
    }

    pub(crate) fn when(&self, event: &CdpEvent) -> Instant {
        let monotonic = event.params.get("timestamp").and_then(Value::as_f64);
        let (Some(offset), Some(monotonic)) = (self.offset_s, monotonic) else {
            return self.read_at;
        };
        let age_ms = self.page_wall_ms - (monotonic + offset) * 1_000.0;
        let age = Duration::from_secs_f64((age_ms / 1_000.0).clamp(0.0, LONGEST_AGE.as_secs_f64()));
        self.read_at.checked_sub(age).unwrap_or(self.read_at)
    }
}

#[cfg(test)]
#[path = "load_settle_tests.rs"]
mod tests;
