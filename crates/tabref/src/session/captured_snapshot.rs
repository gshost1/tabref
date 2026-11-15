use std::collections::HashMap;

use serde_json::Value;
use tabref_core::AccessibilityNode;
use tabref_page::{PageInfo, SkippedFrame};

/// A snapshot read and saved: its id, the tree with refs allocated, the
/// page's address and title, how many refs it holds, how many frames could
/// not be read, and every frame left out of the tree with its reason (ads
/// and trackers that can show nothing, empty frames, timed-out frames), for
/// the view to fold into one line, and the DOM capture the snapshot took of
/// each target it kept, by target id, which the page view reads instead of
/// capturing again.
#[derive(Debug, Clone)]
pub(crate) struct CapturedSnapshot {
    pub(crate) snapshot_id: String,
    pub(crate) tree: AccessibilityNode,
    pub(crate) info: PageInfo,
    pub(crate) ref_count: usize,
    pub(crate) unreadable_frames: usize,
    pub(crate) skipped_frames: Vec<SkippedFrame>,
    pub(crate) dom_captures: HashMap<String, Value>,
}
