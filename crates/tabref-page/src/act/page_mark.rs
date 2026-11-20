pub(crate) type TextBlocks = Vec<(u64, Option<Vec<String>>)>;

/// The page as an action found it: its address, title, and lines of
/// rendered text by block. A missing block value means it was not read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageMark {
    pub url: String,
    pub title: String,
    pub(crate) blocks: TextBlocks,
}
