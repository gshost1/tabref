use tabref_core::AccessibilityNode;
use tabref_page::PageInfo;

/// A snapshot read and saved: its id, the tree with refs allocated, the
/// page's address and title, how many refs it holds and how many frames
/// could not be read.
#[derive(Debug, Clone)]
pub(crate) struct CapturedSnapshot {
    pub(crate) snapshot_id: String,
    pub(crate) tree: AccessibilityNode,
    pub(crate) info: PageInfo,
    pub(crate) ref_count: usize,
    pub(crate) unreadable_frames: usize,
}
