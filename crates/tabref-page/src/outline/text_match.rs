use super::control::Control;

/// One place a text search found its text: the headings the place sits
/// under (outermost first), the block's text around the match, the control
/// the match is in or nearest to, whether the page hides it, and the
/// control that shows it when it is hidden behind one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextMatch {
    pub section: Vec<String>,
    pub line: String,
    pub control: Option<Control>,
    pub hidden: bool,
    pub expander: Option<Control>,
}
