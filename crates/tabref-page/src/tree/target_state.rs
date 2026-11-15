use std::collections::HashMap;

use tabref_cdp::FrameEntry;

use super::cdp_ax_node::CdpAxNode;
use super::frame_dom::FrameDom;
use crate::PageSession;

/// One target a snapshot reads (the page, or an out-of-process iframe), and
/// what its replies brought so far: its frames, its DOM capture, the AX
/// nodes of each frame (the root frame under `""`, since it is asked for
/// before its id is known), and the owner element of each frame whose
/// owner lives in this target's DOM. `parent` is the target that holds this
/// one and the frame of that target it sits in.
#[derive(Debug, Clone)]
pub(crate) struct TargetState {
    pub(crate) session: PageSession,
    pub(crate) parent: Option<(usize, Option<String>)>,
    pub(crate) url: String,
    pub(crate) frames: Option<Vec<FrameEntry>>,
    pub(crate) dom: Option<FrameDom>,
    pub(crate) ax: HashMap<String, Vec<CdpAxNode>>,
    pub(crate) owners: HashMap<String, Option<i64>>,
}

impl TargetState {
    pub(crate) fn new(
        session: PageSession,
        parent: Option<(usize, Option<String>)>,
        url: String,
    ) -> Self {
        Self {
            session,
            parent,
            url,
            frames: None,
            dom: None,
            ax: HashMap::new(),
            owners: HashMap::new(),
        }
    }

    /// The address of `frame_id` when this target lists it, else the
    /// target's own.
    pub(crate) fn frame_url(&self, frame_id: &str) -> String {
        self.frames
            .as_ref()
            .and_then(|frames| frames.iter().find(|frame| frame.frame_id == frame_id))
            .map(|frame| frame.url.clone())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| self.url.clone())
    }
}
