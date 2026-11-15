use tabref_core::AccessibilityNode;

/// A node of a snapshot tree in depth-first pre-order with its parent's
/// index and the end of its subtree, so ancestry and tree distance are
/// cheap to ask: the subtree of `i` is `i..end`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IndexedNode<'a> {
    pub(crate) node: &'a AccessibilityNode,
    pub(crate) parent: Option<usize>,
    pub(crate) end: usize,
    pub(crate) depth: usize,
}

/// Every node of `root` in depth-first pre-order.
pub(crate) fn index_tree(root: &AccessibilityNode) -> Vec<IndexedNode<'_>> {
    let mut nodes = Vec::new();
    visit(root, None, 0, &mut nodes);
    nodes
}

fn visit<'a>(
    node: &'a AccessibilityNode,
    parent: Option<usize>,
    depth: usize,
    nodes: &mut Vec<IndexedNode<'a>>,
) {
    let index = nodes.len();
    nodes.push(IndexedNode {
        node,
        parent,
        end: index + 1,
        depth,
    });
    for child in &node.children {
        visit(child, Some(index), depth + 1, nodes);
    }
    let end = nodes.len();
    nodes[index].end = end;
}

/// Whether `inner` lies in the subtree of `outer`, itself included.
pub(crate) fn within(nodes: &[IndexedNode<'_>], outer: usize, inner: usize) -> bool {
    outer <= inner && inner < nodes[outer].end
}

/// The number of edges between two nodes.
pub(crate) fn distance(nodes: &[IndexedNode<'_>], left: usize, right: usize) -> usize {
    let (mut a, mut b) = (left, right);
    let mut steps = 0;
    while a != b {
        if nodes[a].depth >= nodes[b].depth {
            match nodes[a].parent {
                Some(parent) => a = parent,
                None => return usize::MAX,
            }
        } else {
            match nodes[b].parent {
                Some(parent) => b = parent,
                None => return usize::MAX,
            }
        }
        steps += 1;
    }
    steps
}

/// `indices` without any node that holds another of them, so a cell does
/// not compete with the link inside it.
pub(crate) fn deepest(nodes: &[IndexedNode<'_>], indices: &[usize]) -> Vec<usize> {
    indices
        .iter()
        .copied()
        .filter(|&outer| {
            !indices
                .iter()
                .any(|&inner| inner != outer && within(nodes, outer, inner))
        })
        .collect()
}
