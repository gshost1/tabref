use std::time::Duration;

use tabref_core::Deadline;
use tabref_page::{LoadSettle, LoadWatch, SettleScope, settle_load};

use crate::session::Session;

const CAPTURE_RESERVE: Duration = Duration::from_millis(1_000);

/// The line an open's view ends with when the page was near-empty and
/// still fetching or changing after the longer wait.
pub(crate) const STILL_RENDERING: &str =
    "note: the page was still rendering; tabref wait --text <words you expect>, then tabref snap";

/// After a navigation `watch` followed, lets the page's own load-time work
/// settle when the page loaded and nothing downloaded, keeping a second of
/// `deadline` for the capture after it; otherwise just ends the watch. The
/// plain wait is made only when this command keeps its capture for reuse
/// (a daemon read); a near-empty page is waited for either way.
pub(crate) fn settle_after(
    session: &mut Session,
    watch: Option<LoadWatch>,
    loaded: bool,
    deadline: Deadline,
) -> Option<LoadSettle> {
    let watch = watch?;
    if !loaded {
        watch.stop(session.connection(), deadline);
        return None;
    }
    let scope = SettleScope {
        plain: session.keeps_captures(),
        reserve: CAPTURE_RESERVE,
    };
    Some(settle_load(session.connection(), watch, deadline, scope))
}
