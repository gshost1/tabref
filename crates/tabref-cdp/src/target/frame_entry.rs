/// One frame of a target's `Page.getFrameTree`: its id, its parent frame
/// (absent only for a page's main frame) and the loader of the document it
/// shows now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameEntry {
    pub frame_id: String,
    pub parent_id: Option<String>,
    pub loader_id: String,
}
