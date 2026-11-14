/// What the budget left out of a view: lines and characters in all and
/// shown, chrome lines dropped (navigation, header, footer and sidebar
/// past their first lines), and the sections left with only their heading,
/// with the first of them by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CutReport {
    pub(crate) lines_total: usize,
    pub(crate) lines_shown: usize,
    pub(crate) chars_total: usize,
    pub(crate) chars_shown: usize,
    pub(crate) chrome_cut: usize,
    pub(crate) sections_cut: usize,
    pub(crate) first_cut_heading: Option<String>,
}

impl CutReport {
    pub(crate) fn is_complete(&self) -> bool {
        self.lines_shown >= self.lines_total && self.chrome_cut == 0
    }
}
