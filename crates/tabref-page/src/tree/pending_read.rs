/// A snapshot request in flight, by the target (an index into the reader's
/// targets) it serves. `Ax` with an empty `frame_id` asks for the target's
/// root frame; `Owner` asks target `target` for the element that holds
/// `frame_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingRead {
    AutoAttach(usize),
    FrameTree(usize),
    Dom(usize),
    Ax { target: usize, frame_id: String },
    Owner { target: usize, frame_id: String },
}

impl PendingRead {
    pub(crate) fn target(&self) -> usize {
        match self {
            Self::AutoAttach(target) | Self::FrameTree(target) | Self::Dom(target) => *target,
            Self::Ax { target, .. } | Self::Owner { target, .. } => *target,
        }
    }

    /// Whether the page itself waits on this read: the page target's frame
    /// list, DOM capture, main-frame tree and auto-attach. Its in-process
    /// iframes and frame owners are iframe reads.
    pub(crate) fn is_page_read(&self) -> bool {
        match self {
            Self::Ax { target, frame_id } => *target == 0 && frame_id.is_empty(),
            Self::Owner { .. } => false,
            other => other.target() == 0,
        }
    }
}
