use tabref_cdp::PageSession;

use super::focus_state::FocusState;

/// A tab's flattened session as a [`Session`](super::Session) keeps it, and
/// whether the tab was told it holds the window focus.
#[derive(Debug, Clone)]
pub(crate) struct AttachedTab {
    pub(crate) session: PageSession,
    pub(crate) focus: FocusState,
}
