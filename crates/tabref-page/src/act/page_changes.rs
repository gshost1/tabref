use super::text_change::TextChange;

/// The page after an action, against the [`PageMark`](super::page_mark::PageMark)
/// taken before it: its address and title now, how many DOM mutations the
/// action caused (`None` when the document was replaced, so nothing was
/// watched), how many shown frames from other sites the check could not
/// read, and the lines of rendered text that changed, in page order: the
/// document's, then those of its open shadow roots and same-origin frames.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageChanges {
    pub url: String,
    pub title: String,
    pub mutations: Option<u64>,
    pub unread_frames: u64,
    /// Text blocks over budget in either scan. The main document has its
    /// own budget, separate from frames and shadow roots.
    pub unchecked_text_blocks: usize,
    pub changes: Vec<TextChange>,
    /// Whole body replacements: old row count and new rows, also appended to `changes`.
    pub replaced_rows: Vec<(usize, Vec<String>)>,
}

impl PageChanges {
    /// Whether the action changed anything a reader of the page could see
    /// or a DOM observer could count.
    pub fn changed_anything(&self) -> bool {
        !self.changes.is_empty() || self.mutations.is_none_or(|count| count > 0)
    }
}
