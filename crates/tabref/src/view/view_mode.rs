/// How much of a page a snapshot prints. `Interactive` shows controls,
/// headings, dialogs and alerts, and the named regions that hold controls;
/// `All` adds every node that carries text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewMode {
    Interactive,
    All,
}

impl ViewMode {
    pub(crate) fn from_all(all: bool) -> Self {
        if all { Self::All } else { Self::Interactive }
    }

    /// The longest name a line prints before it is cut.
    pub(crate) fn name_limit(self) -> usize {
        match self {
            Self::Interactive => 160,
            Self::All => 2_000,
        }
    }
}
