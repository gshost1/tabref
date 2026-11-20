/// The sections past the budget's cut: how many there are and how many
/// the outline lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SectionCut {
    pub(crate) count: usize,
    pub(crate) listed: usize,
}
