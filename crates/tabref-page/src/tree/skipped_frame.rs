use serde::Serialize;

use super::frame_skip_reason::FrameSkipReason;

/// A frame left out of a snapshot: its frame (or iframe target) id, the
/// address it showed when known, and why. A view can fold these into one
/// line such as "3 frames skipped (2 tiny, 1 timed out)".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedFrame {
    pub frame_id: String,
    pub url: String,
    pub reason: FrameSkipReason,
}
