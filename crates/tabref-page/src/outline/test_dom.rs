//! Builds small captured documents for the outline's unit tests.

use super::dom_node::DomNode;
use super::dom_tree::DomTree;
use super::form_state::FormState;
use super::node_kind::NodeKind;
use super::node_layout::NodeLayout;

/// A document under construction. Every node gets backend id `index + 1`.
pub(crate) struct TestDom {
    pub(crate) tree: DomTree,
}

impl TestDom {
    /// A document with `<html>` and a shown `<body>` (index 2).
    pub(crate) fn new() -> Self {
        let mut dom = Self {
            tree: DomTree {
                target_id: "T1".into(),
                frame_id: "F1".into(),
                url: "http://site.test/page".into(),
                base_url: "http://site.test/page".into(),
                nodes: Vec::new(),
            },
        };
        dom.push(None, NodeKind::Document, &[], Some("block"));
        dom.push(
            Some(0),
            NodeKind::Element("HTML".into()),
            &[],
            Some("block"),
        );
        dom.push(
            Some(1),
            NodeKind::Element("BODY".into()),
            &[],
            Some("block"),
        );
        dom
    }

    /// Adds an element under `parent`; `display` `None` means not rendered.
    pub(crate) fn element(
        &mut self,
        parent: usize,
        tag: &str,
        attrs: &[(&str, &str)],
        display: Option<&str>,
    ) -> usize {
        self.push(
            Some(parent),
            NodeKind::Element(tag.to_ascii_uppercase()),
            attrs,
            display,
        )
    }

    /// Adds a text node under `parent`, rendered when `shown`.
    pub(crate) fn text(&mut self, parent: usize, text: &str, shown: bool) -> usize {
        self.push(
            Some(parent),
            NodeKind::Text(text.into()),
            &[],
            shown.then_some("inline"),
        )
    }

    /// Gives the rendered node at `index` `visibility: hidden`.
    pub(crate) fn hide(&mut self, index: usize) {
        if let Some(layout) = self.tree.nodes[index].layout.as_mut() {
            layout.visible = false;
        }
    }

    pub(crate) fn form(&mut self, index: usize) -> &mut FormState {
        &mut self.tree.nodes[index].form
    }

    fn push(
        &mut self,
        parent: Option<usize>,
        kind: NodeKind,
        attrs: &[(&str, &str)],
        display: Option<&str>,
    ) -> usize {
        let index = self.tree.nodes.len();
        self.tree.nodes.push(DomNode {
            parent,
            children: Vec::new(),
            kind,
            backend_id: i64::try_from(index).unwrap_or(0) + 1,
            attrs: attrs
                .iter()
                .map(|(name, value)| ((*name).into(), (*value).into()))
                .collect(),
            layout: display.map(|display| NodeLayout {
                display: display.into(),
                position: "static".into(),
                visible: true,
                bounds: [0.0, 0.0, 100.0, 20.0],
            }),
            form: FormState::default(),
        });
        if let Some(parent) = parent {
            self.tree.nodes[parent].children.push(index);
        }
        index
    }
}
