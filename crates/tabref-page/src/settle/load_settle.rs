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
pub const LOAD_SETTLE_CAP: Duration = Duration::from_millis(300);
/// How long after `load` a near-empty page is waited for while a fetch of
/// its own site that was already in flight at `load` has not finished.
pub const RENDER_CAP: Duration = Duration::from_millis(1_500);
const NETWORK_QUIET: Duration = Duration::from_millis(50);
const DOM_QUIET: Duration = Duration::from_millis(50);
const LAST_READ: Duration = Duration::from_millis(30);
const LONGEST_AGE: Duration = Duration::from_secs(10);
const TRACKED: &[&str] = &["XHR", "Fetch"];

/// How the page stood when the wait ended: whether its work settled
/// (no fetch of its own site in flight, 50 ms without one and 50 ms
/// without a DOM change), whether it was near-empty, whether it was
/// near-empty with a fetch from its load still in flight, whether the main
/// frame began another navigation, and how long it took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSettle {
    pub settled: bool,
    pub near_empty: bool,
    pub rendering: bool,
    pub navigated: bool,
    pub waited: Duration,
}

impl LoadSettle {
    /// A near-empty page still fetching what its load asked for when the
    /// longer wait gave up.
    pub fn still_rendering(&self) -> bool {
        self.rendering
    }
}

/// Which wait [`settle_load`] makes: `plain` waits for the page's work up
/// to [`LOAD_SETTLE_CAP`], worth it only when the capture after it is kept
/// for reuse; without it only a near-empty page is waited for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettleScope {
    pub plain: bool,
    pub reserve: Duration,
}

/// Waits, after `load`, until the page's XHR and fetch requests to its own
/// site have finished and neither they nor the DOM have changed for 50 ms,
/// for at most 300 ms (a read at the bound may take 30 ms more). A page
/// that is near-empty then (see [`LoadStatus`]) and still waits on a fetch
/// that was in flight at `load` gets up to 1.5 s in all, for that fetch
/// alone: DOM changes and later requests (a clock, a poll) never extend
/// it. None of it eats into the last `reserve` of `deadline`. A new
/// navigation of the main frame, or a page that cannot be read, ends the
/// wait. Ends the watch either way.
pub fn settle_load(
    connection: &mut CdpConnection,
    mut watch: LoadWatch,
    deadline: Deadline,
    scope: SettleScope,
) -> LoadSettle {
    let started = Instant::now();
    watch.forget_script(connection, deadline);
    let bound = deadline.capped(deadline.remaining().saturating_sub(scope.reserve));
    let cap = if scope.plain {
        LOAD_SETTLE_CAP
    } else {
        Duration::ZERO
    };
    let outcome = follow(connection, &watch, (started, cap), bound).unwrap_or(LoadSettle {
        settled: false,
        near_empty: false,
        rendering: false,
        navigated: false,
        waited: started.elapsed(),
    });
    watch.stop(connection, deadline);
    outcome
}

fn follow(
    connection: &mut CdpConnection,
    watch: &LoadWatch,
    (started, cap): (Instant, Duration),
    deadline: Deadline,
) -> Result<LoadSettle, AppError> {
    let session_id = watch.session.session_id.clone();
    let read_by = |cap: Duration| {
        let left = (started + cap).saturating_duration_since(Instant::now());
        deadline.capped(left.max(LAST_READ))
    };
    let mut status = watch.status(connection, read_by(cap))?;
    let mut clock = EventClock::new(status.wall_ms, Instant::now());
    let mut feed = Feed::new(&watch.session.target_id);
    for event in connection.drain_events(|event| is_network_event(event, &session_id)) {
        clock.calibrate(&event);
        feed.absorb(&event, clock.when(&event), true);
    }
    let mut changed = changed_at(&status, Instant::now());
    let end = |settled, status: &LoadStatus, feed: &Feed| LoadSettle {
        settled,
        near_empty: status.near_empty,
        rendering: false,
        navigated: feed.navigated,
        waited: started.elapsed(),
    };
    loop {
        let now = Instant::now();
        let network = feed.all.next_wait(now, NETWORK_QUIET);
        let dom_quiet_at = changed.map(|at| at + DOM_QUIET).filter(|at| *at > now);
        if feed.navigated {
            return Ok(end(false, &status, &feed));
        }
        if network == SettleWait::Done && dom_quiet_at.is_none() {
            return Ok(end(true, &status, &feed));
        }
        if now >= started + cap || deadline.is_expired() {
            if !deadline.is_expired() {
                if !cap.is_zero() {
                    status = watch.status(connection, read_by(cap))?;
                }
                if status.near_empty && fetching(&feed.initial, Instant::now()) {
                    return render(connection, watch, &mut feed, started, deadline);
                }
            }
            return Ok(end(false, &status, &feed));
        }
        let until = match network {
            SettleWait::Network | SettleWait::Navigation => started + cap,
            SettleWait::QuietUntil(at) => at,
            SettleWait::Done => dom_quiet_at.unwrap_or(now),
        };
        let until = until.min(started + cap).max(now);
        match next(connection, &session_id, deadline.capped(until - now))? {
            Some(event) => feed.absorb(&event, Instant::now(), false),
            None => {
                status = watch.status(connection, read_by(cap))?;
                changed = changed_at(&status, Instant::now());
            }
        }
    }
}

/// The longer wait of a near-empty page: until the fetches in flight at
/// `load` finish and 50 ms pass, or 1.5 s after `load`.
fn render(
    connection: &mut CdpConnection,
    watch: &LoadWatch,
    feed: &mut Feed,
    started: Instant,
    deadline: Deadline,
) -> Result<LoadSettle, AppError> {
    let session_id = watch.session.session_id.clone();
    let limit = started + RENDER_CAP;
    loop {
        let now = Instant::now();
        let waiting = fetching(&feed.initial, now);
        if feed.navigated || !waiting || now >= limit || deadline.is_expired() {
            let read = deadline.capped(limit.saturating_duration_since(now).max(LAST_READ));
            let status = watch.status(connection, read)?;
            return Ok(LoadSettle {
                settled: !waiting,
                near_empty: status.near_empty,
                rendering: waiting && status.near_empty && !feed.navigated,
                navigated: feed.navigated,
                waited: started.elapsed(),
            });
        }
        let until = match feed.initial.next_wait(now, NETWORK_QUIET) {
            SettleWait::QuietUntil(at) => at.min(limit),
            _ => limit,
        };
        if let Some(event) = next(connection, &session_id, deadline.capped(until - now))? {
            feed.absorb(&event, Instant::now(), false);
        }
    }
}

fn fetching(tracker: &SettleTracker, now: Instant) -> bool {
    matches!(
        tracker.next_wait(now, NETWORK_QUIET),
        SettleWait::Network | SettleWait::QuietUntil(_)
    )
}

fn changed_at(status: &LoadStatus, now: Instant) -> Option<Instant> {
    status
        .since_mutation
        .map(|since| now.checked_sub(since).unwrap_or(now))
}

/// The network events of the watched page, folded twice: `all` follows
/// every XHR and fetch request to the page's own site, `initial` only those
/// already sent when the wait began (the load's own). A document request is
/// the page itself or a frame, which `load` already waited for, or an ad
/// slot added later, so it is never followed; a second document request of
/// the main frame is a new navigation, which ends the wait.
struct Feed {
    all: SettleTracker,
    initial: SettleTracker,
    main_frame: String,
    document: Option<String>,
    navigated: bool,
}

impl Feed {
    fn new(main_frame: &str) -> Self {
        Self {
            all: SettleTracker::new(main_frame),
            initial: SettleTracker::new(main_frame),
            main_frame: main_frame.to_owned(),
            document: None,
            navigated: false,
        }
    }

    fn absorb(&mut self, event: &CdpEvent, at: Instant, buffered: bool) {
        if event.method == "Network.requestWillBeSent" {
            let params = &event.params;
            let kind = params.get("type").and_then(Value::as_str);
            if kind == Some("Document") {
                self.note_document(params);
                return;
            }
            if !kind.is_some_and(|kind| TRACKED.contains(&kind)) {
                return;
            }
            if buffered {
                self.initial.absorb(event, at);
            }
            self.all.absorb(event, at);
            return;
        }
        self.initial.absorb(event, at);
        self.all.absorb(event, at);
    }

    fn note_document(&mut self, params: &Value) {
        let frame = params.get("frameId").and_then(Value::as_str);
        if frame != Some(self.main_frame.as_str()) {
            return;
        }
        let id = params
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match &self.document {
            None => self.document = Some(id.to_owned()),
            Some(first) if first != id => self.navigated = true,
            Some(_) => {}
        }
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
