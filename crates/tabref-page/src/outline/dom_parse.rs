use std::collections::HashMap;

use serde_json::Value;

use super::capture_strings::Strings;
use super::dom_node::DomNode;
use super::dom_tree::DomTree;
use super::node_kind::NodeKind;
use super::node_layout::NodeLayout;
use super::rare_data::RareData;

/// The computed styles a capture asks for, in the order its layout reports
/// them.
pub(crate) const CAPTURED_STYLES: [&str; 3] = ["display", "visibility", "position"];

/// Reads a `DOMSnapshot.captureSnapshot` reply into one tree per document.
/// Names, values and attributes are indexes into the reply's shared string
/// table; a node with no layout entry is not rendered.
pub fn parse_capture(reply: &Value, target_id: &str) -> Vec<DomTree> {
    let empty = Vec::new();
    let strings: Vec<&str> = reply
        .get("strings")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .map(|value| value.as_str().unwrap_or_default())
        .collect();
    let table = Strings(&strings);
    reply
        .get("documents")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .map(|document| parse_document(document, &table, target_id))
        .collect()
}

fn parse_document(document: &Value, strings: &Strings<'_>, target_id: &str) -> DomTree {
    let nodes = document.get("nodes").unwrap_or(&Value::Null);
    let column = |name: &str| {
        nodes
            .get(name)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let parents = column("parentIndex");
    let types = column("nodeType");
    let names = column("nodeName");
    let values = column("nodeValue");
    let backends = column("backendNodeId");
    let attributes = column("attributes");
    let rare = RareData::read(nodes, strings);
    let layouts = read_layouts(document.get("layout").unwrap_or(&Value::Null), strings);
    let mut built: Vec<DomNode> = (0..parents.len())
        .map(|index| {
            let parent = parents
                .get(index)
                .and_then(Value::as_i64)
                .and_then(|parent| usize::try_from(parent).ok());
            let kind = match types.get(index).and_then(Value::as_i64) {
                Some(1) => NodeKind::Element(strings.text(names.get(index)).to_ascii_uppercase()),
                Some(3) => NodeKind::Text(strings.text(values.get(index))),
                Some(9) => NodeKind::Document,
                _ => NodeKind::Other,
            };
            DomNode {
                parent,
                children: Vec::new(),
                kind,
                backend_id: backends.get(index).and_then(Value::as_i64).unwrap_or(0),
                attrs: read_attributes(attributes.get(index), strings),
                layout: layouts.get(&index).cloned(),
                form: rare.form(index),
            }
        })
        .collect();
    for index in 0..built.len() {
        if let Some(parent) = built[index].parent.filter(|parent| *parent < index) {
            built[parent].children.push(index);
        }
    }
    DomTree {
        target_id: target_id.to_owned(),
        frame_id: strings.text(document.get("frameId")),
        url: strings.text(document.get("documentURL")),
        base_url: strings.text(document.get("baseURL")),
        nodes: built,
    }
}

fn read_attributes(pairs: Option<&Value>, strings: &Strings<'_>) -> Vec<(String, String)> {
    let Some(pairs) = pairs.and_then(Value::as_array) else {
        return Vec::new();
    };
    pairs
        .chunks(2)
        .filter_map(|pair| {
            let name = strings.get(pair.first()?)?;
            let value = pair.get(1).and_then(|value| strings.get(value));
            Some((
                name.to_ascii_lowercase(),
                value.unwrap_or_default().to_owned(),
            ))
        })
        .collect()
}

fn read_layouts(layout: &Value, strings: &Strings<'_>) -> HashMap<usize, NodeLayout> {
    let empty = Vec::new();
    let array = |name: &str| layout.get(name).and_then(Value::as_array).unwrap_or(&empty);
    let (indexes, styles, bounds) = (array("nodeIndex"), array("styles"), array("bounds"));
    let mut layouts = HashMap::with_capacity(indexes.len());
    for (position, node) in indexes.iter().enumerate() {
        let Some(node) = node.as_u64().and_then(|node| usize::try_from(node).ok()) else {
            continue;
        };
        if layouts.contains_key(&node) {
            continue;
        }
        let style = styles.get(position).and_then(Value::as_array);
        let style_at = |slot: usize| {
            style
                .and_then(|style| style.get(slot))
                .and_then(|index| strings.get(index))
                .unwrap_or_default()
                .to_owned()
        };
        let mut rect = [0.0; 4];
        if let Some(values) = bounds.get(position).and_then(Value::as_array) {
            for (slot, value) in values.iter().take(4).enumerate() {
                rect[slot] = value.as_f64().unwrap_or(0.0);
            }
        }
        let visibility = style_at(1);
        layouts.insert(
            node,
            NodeLayout {
                display: style_at(0),
                position: style_at(2),
                visible: visibility != "hidden" && visibility != "collapse",
                bounds: rect,
            },
        );
    }
    layouts
}

#[cfg(test)]
#[path = "dom_parse_tests.rs"]
mod tests;
