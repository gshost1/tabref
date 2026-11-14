/// Where the outline walk is: the capture and document it reads, how deep
/// it has gone, whether a sectioning element holds the current node, and
/// whether a `<pre>` or a `main` landmark does, and how many frames deep
/// it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WalkContext {
    pub(crate) capture: usize,
    pub(crate) document: usize,
    pub(crate) depth: usize,
    pub(crate) in_section: bool,
    pub(crate) in_pre: bool,
    pub(crate) in_main: bool,
    pub(crate) frame_depth: usize,
}

impl WalkContext {
    pub(crate) fn deeper(&self) -> Self {
        Self {
            depth: self.depth + 1,
            ..*self
        }
    }
}
