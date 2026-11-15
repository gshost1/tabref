use super::block::Block;
use super::frame_stats::FrameStats;

/// A page read into blocks: the body in reading order, the dialogs and
/// overlays lifted out of it (they print first), and what happened to the
/// page's frames.
#[derive(Debug, Clone, PartialEq)]
pub struct PageOutline {
    pub body: Block,
    pub dialogs: Vec<Block>,
    pub frames: FrameStats,
}
