/// The sections past the budget's cut: how many, how many the outline after
/// the cut lists, and the first one's plain heading text, which `read
/// --section` matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SectionCut {
    pub(crate) count: usize,
    pub(crate) listed: usize,
    pub(crate) first_heading: Option<String>,
}
