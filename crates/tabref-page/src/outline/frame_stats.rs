/// What the outline did with the page's frames: how many it showed, and
/// how many it left out because they were too small to hold content
/// (tracking pixels, hidden slots), cross-origin with nothing to fill in
/// (ads, widgets), empty, or not read at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub shown: usize,
    pub tiny: usize,
    pub cross_origin: usize,
    pub empty: usize,
    pub unread: usize,
}

impl FrameStats {
    pub fn skipped(&self) -> usize {
        self.tiny + self.cross_origin + self.empty + self.unread
    }
}
