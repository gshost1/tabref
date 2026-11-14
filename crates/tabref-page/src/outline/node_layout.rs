/// How the renderer laid a node out: its computed `display` and `position`,
/// whether `visibility` lets it show, and its box `[x, y, width, height]` in
/// CSS pixels of its document. Only rendered nodes have a layout.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeLayout {
    pub display: String,
    pub position: String,
    pub visible: bool,
    pub bounds: [f64; 4],
}

impl NodeLayout {
    pub fn area(&self) -> f64 {
        self.bounds[2].max(0.0) * self.bounds[3].max(0.0)
    }

    /// At most one pixel in either direction: a tracking pixel or text
    /// hidden for screen readers only.
    pub fn is_tiny(&self) -> bool {
        self.bounds[2] <= 1.0 || self.bounds[3] <= 1.0
    }
}
