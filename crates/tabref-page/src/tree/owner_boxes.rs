use std::collections::{HashMap, HashSet};

use super::frame_skip_reason::FrameSkipReason;
use super::owner_box::OwnerBox;

/// What one DOM capture says about the elements that may own frames: every
/// node it holds, and the box of every node it laid out. Any element type
/// can own a frame (`<iframe>`, `<frame>`, `<object>`, `<embed>`,
/// `<fencedframe>`), so none is singled out.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct OwnerBoxes {
    pub(crate) present: HashSet<i64>,
    pub(crate) boxes: HashMap<i64, OwnerBox>,
}

impl OwnerBoxes {
    /// Why the frame `backend` owns can show nothing: its box says so, or
    /// the capture holds the owner but laid it out nowhere. An owner the
    /// capture does not hold says nothing, and the frame is kept.
    pub(crate) fn skip_reason(&self, backend: i64) -> Option<FrameSkipReason> {
        match self.boxes.get(&backend) {
            Some(owner) => owner.skip_reason(),
            None if self.present.contains(&backend) => Some(FrameSkipReason::Hidden),
            None => None,
        }
    }
}
