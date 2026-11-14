use std::time::Instant;

/// What the settle loop waits for next: nothing, a navigation to commit and
/// reach `DOMContentLoaded`, requests in flight, or the end of the quiet
/// window after the last request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettleWait {
    Done,
    Navigation,
    Network,
    QuietUntil(Instant),
}
