/// How much of a page the accessibility-tree lines print: controls,
/// headings, dialogs and alerts, and the named regions that hold controls.
/// Action results print the page this way; the page view itself is built
/// from the DOM (see `page_view`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewMode {
    Interactive,
}

impl ViewMode {
    /// The longest name a line prints before it is cut.
    pub(crate) fn name_limit(self) -> usize {
        match self {
            Self::Interactive => 160,
        }
    }
}
