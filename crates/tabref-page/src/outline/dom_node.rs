use super::form_state::FormState;
use super::node_kind::NodeKind;
use super::node_layout::NodeLayout;

/// One node of a captured document: its parent and children as indexes
/// into the document's nodes, what it is, its backend node id, its
/// attributes in source order, its layout when rendered, and its form state.
#[derive(Debug, Clone, PartialEq)]
pub struct DomNode {
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub kind: NodeKind,
    pub backend_id: i64,
    pub attrs: Vec<(String, String)>,
    pub layout: Option<NodeLayout>,
    pub form: FormState,
}

impl DomNode {
    /// The upper-case tag of an element, `""` for any other node.
    pub fn tag(&self) -> &str {
        match &self.kind {
            NodeKind::Element(tag) => tag,
            _ => "",
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Whether the renderer shows the node: it has a layout whose
    /// visibility is not hidden.
    pub fn is_shown(&self) -> bool {
        self.layout.as_ref().is_some_and(|layout| layout.visible)
    }

    pub fn display(&self) -> &str {
        self.layout
            .as_ref()
            .map_or("none", |layout| layout.display.as_str())
    }
}
