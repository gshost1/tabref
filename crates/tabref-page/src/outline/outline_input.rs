use std::collections::HashMap;

use super::dom_tree::DomTree;
use super::ref_label::RefLabel;

/// What an outline is built from: the documents the page's own target
/// renders (main document first, then its in-process iframes), the
/// documents of out-of-process iframes keyed by their owner element
/// (target id and backend node id of the `<iframe>`), the refs the
/// accessibility snapshot gave, keyed the same way, and the viewport size
/// in CSS pixels.
#[derive(Debug, Clone, Default)]
pub struct OutlineInput {
    pub trees: Vec<DomTree>,
    pub frames: HashMap<(String, i64), Vec<DomTree>>,
    pub refs: HashMap<(String, i64), RefLabel>,
    pub viewport: [f64; 2],
}
