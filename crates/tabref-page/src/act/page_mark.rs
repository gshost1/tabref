/// The page as an action found it: its address, its title and its rendered
/// text as non-empty lines, or `None` when the text was too long to keep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageMark {
    pub url: String,
    pub title: String,
    pub lines: Option<Vec<String>>,
}
