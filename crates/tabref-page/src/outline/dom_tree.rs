use super::dom_node::DomNode;

/// One captured document: the target whose session read it, its frame,
/// its address and base address, and its nodes with the document node at
/// index 0.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DomTree {
    pub target_id: String,
    pub frame_id: String,
    pub url: String,
    pub base_url: String,
    pub nodes: Vec<DomNode>,
}

impl DomTree {
    pub fn node(&self, index: usize) -> Option<&DomNode> {
        self.nodes.get(index)
    }

    /// The index of the node with `backend_id`.
    pub fn find_backend(&self, backend_id: i64) -> Option<usize> {
        self.nodes
            .iter()
            .position(|node| node.backend_id == backend_id)
    }

    /// The index of the `<body>` element, or else the document's root.
    pub fn body(&self) -> usize {
        self.nodes
            .iter()
            .position(|node| node.tag() == "BODY")
            .unwrap_or(0)
    }

    /// `index` and each of its ancestors, nearest first.
    pub fn ancestors(&self, index: usize) -> Vec<usize> {
        let mut chain = vec![index];
        let mut current = index;
        while let Some(parent) = self.nodes.get(current).and_then(|node| node.parent) {
            if chain.contains(&parent) {
                break;
            }
            chain.push(parent);
            current = parent;
        }
        chain
    }
}
