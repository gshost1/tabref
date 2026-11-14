//! The time a snapshot gives its iframes. Every frame is read at once, so the
//! iframe phase lasts as long as its slowest frame; one that stalls (an ad
//! renderer busy with script) would hold the whole snapshot. A budgeted read
//! waits for iframes until a short while after the page itself is read (at
//! least 300 ms from the start, and 100 ms past the page), never past a
//! reserve short of the deadline, and leaves out the frames still pending.

use std::time::{Duration, Instant};

use tabref_core::{AppError, Deadline};

use tabref_cdp::is_timeout;

const RESERVE_SHARE: u32 = 5;
const MINIMUM_RESERVE: Duration = Duration::from_millis(100);
const FLOOR: Duration = Duration::from_millis(300);
const GRACE: Duration = Duration::from_millis(100);

#[derive(Clone, Copy)]
pub(crate) struct FrameBudget {
    snapshot: Deadline,
    cap: Deadline,
    started: Instant,
    leaves_out_late_iframes: bool,
}

impl FrameBudget {
    /// Iframes share the snapshot deadline, and one that misses it fails the
    /// snapshot.
    pub(crate) fn whole(deadline: Deadline) -> Self {
        Self {
            snapshot: deadline,
            cap: deadline,
            started: Instant::now(),
            leaves_out_late_iframes: false,
        }
    }

    /// Iframes get at most what remains of `deadline` when the read starts,
    /// less a fifth of it (at least 100 ms) kept for detaching the iframe
    /// sessions and building the tree, and once the page is read, only the
    /// short window [`iframes`](Self::iframes) describes.
    pub(crate) fn reserved(deadline: Deadline) -> Self {
        let remaining = deadline.remaining();
        let reserve = (remaining / RESERVE_SHARE).max(MINIMUM_RESERVE);
        Self {
            snapshot: deadline,
            cap: deadline.capped(remaining.saturating_sub(reserve)),
            started: Instant::now(),
            leaves_out_late_iframes: true,
        }
    }

    pub(crate) fn snapshot(self) -> Deadline {
        self.snapshot
    }

    /// Until when iframe reads are waited for, given when the page's own
    /// reads finished (`None` while they are still in flight).
    pub(crate) fn iframes(self, page_done: Option<Instant>) -> Deadline {
        if !self.leaves_out_late_iframes {
            return self.snapshot;
        }
        let Some(done) = page_done else {
            return self.cap;
        };
        let until = (self.started + FLOOR).max(done + GRACE);
        self.cap
            .capped(until.saturating_duration_since(Instant::now()))
    }

    /// Whether the iframe budget is spent, so no further iframe is read.
    pub(crate) fn is_spent(self, page_done: Option<Instant>) -> bool {
        self.leaves_out_late_iframes && self.iframes(page_done).is_expired()
    }

    /// Whether `error` is an iframe read that ran out of its budget while the
    /// snapshot itself still has time.
    pub(crate) fn missed(self, error: &AppError) -> bool {
        self.leaves_out_late_iframes && is_timeout(error) && !self.snapshot.is_expired()
    }
}

#[cfg(test)]
#[path = "frame_budget_tests.rs"]
mod tests;
