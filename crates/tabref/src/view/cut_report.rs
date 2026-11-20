use super::section_cut::SectionCut;

/// What the budget left out of a view: lines and characters in all and
/// shown, chrome lines dropped (navigation, header, footer and sidebar
/// past their first lines), the sections left with only their heading,
/// the number of the first line the budget left out, and the number of
/// the first line after the one `--to` asked the view to stop at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CutReport {
    pub(crate) lines_total: usize,
    pub(crate) lines_shown: usize,
    pub(crate) chars_total: usize,
    pub(crate) chars_shown: usize,
    pub(crate) chrome_cut: usize,
    pub(crate) sections: SectionCut,
    pub(crate) budget_from: Option<usize>,
    pub(crate) after_to: Option<usize>,
}

impl CutReport {
    pub(crate) fn is_complete(&self) -> bool {
        self.budget_from.is_none() && self.after_to.is_none() && self.chrome_cut == 0
    }
}
