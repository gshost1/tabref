/// The sections the budget left with only their heading: how many, and
/// the first one's plain heading text, which `read --section` matches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SectionCut {
    pub(crate) count: usize,
    pub(crate) first_heading: Option<String>,
}
