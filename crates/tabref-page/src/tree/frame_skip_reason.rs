use serde::Serialize;

/// Why a frame is not in a snapshot's tree. The first five say the frame
/// could hold nothing worth showing: its element is not laid out or hidden,
/// has no area, is a pixel (at most 5x5 with its border), lies wholly above or left of the
/// page, or the frame was read and holds no text and no control. The last
/// two say the frame may hold something but was not read: it missed the
/// iframe budget, or the browser refused or lost it. A frame that is shown
/// but could not list the iframes inside it in time is also reported as
/// timed out, since some of its content may be missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameSkipReason {
    Hidden,
    ZeroSize,
    Tiny,
    Offscreen,
    Empty,
    TimedOut,
    Unreadable,
}

impl FrameSkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::ZeroSize => "zero_size",
            Self::Tiny => "tiny",
            Self::Offscreen => "offscreen",
            Self::Empty => "empty",
            Self::TimedOut => "timed_out",
            Self::Unreadable => "unreadable",
        }
    }

    /// Whether the frame was left out without its content being known.
    pub fn is_unread(self) -> bool {
        matches!(self, Self::TimedOut | Self::Unreadable)
    }
}
