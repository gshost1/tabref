use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// When the daemon started, when it last finished a request, and how many
/// requests it is serving now. The daemon exits once it has been idle, with
/// nothing in flight, for its idle timeout.
#[derive(Debug)]
pub(crate) struct Activity {
    started: Instant,
    last_ms: AtomicU64,
    active: AtomicUsize,
}

impl Activity {
    pub(crate) fn new() -> Self {
        Self {
            started: Instant::now(),
            last_ms: AtomicU64::new(0),
            active: AtomicUsize::new(0),
        }
    }

    pub(crate) fn begin(&self) {
        self.active.fetch_add(1, Ordering::SeqCst);
    }

    pub(crate) fn end(&self) {
        self.last_ms.store(self.now_ms(), Ordering::SeqCst);
        self.active.fetch_sub(1, Ordering::SeqCst);
    }

    pub(crate) fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    /// How long the daemon has had nothing to do; zero while it serves.
    pub(crate) fn idle(&self) -> Duration {
        if self.active.load(Ordering::SeqCst) > 0 {
            return Duration::ZERO;
        }
        let since = self
            .now_ms()
            .saturating_sub(self.last_ms.load(Ordering::SeqCst));
        Duration::from_millis(since)
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}
