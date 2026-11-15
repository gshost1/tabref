use super::dom_node::DomNode;

const STATE_CLASSES: &[&str] = &[
    "active",
    "selected",
    "current",
    "open",
    "closed",
    "odd",
    "even",
    "first",
    "last",
    "hidden",
    "show",
    "shown",
    "in",
    "visible",
    "collapsed",
    "expanded",
    "disabled",
    "focus",
    "hover",
];

const KEYED_TAGS: &[&str] = &[
    "LI",
    "TR",
    "ARTICLE",
    "DT",
    "DD",
    "DETAILS",
    "SECTION",
    "FIGURE",
    "BLOCKQUOTE",
];

/// The shape key that marks siblings as repeats of one another: the tag
/// and the element's stable classes. Classes that carry a number or name a
/// passing state (`active`, `odd`) are left out, so the third item of a
/// list keys like the first. An element with no stable class keys by tag
/// only when its tag is itself an item (`<li>`, `<tr>`, `<article>`).
pub(crate) fn repeat_key(node: &DomNode) -> Option<String> {
    let mut classes: Vec<&str> = node
        .attr("class")
        .unwrap_or_default()
        .split_whitespace()
        .filter(|class| !class.chars().any(|c| c.is_ascii_digit()))
        .filter(|class| !is_state_class(class))
        .collect();
    classes.sort_unstable();
    classes.dedup();
    if classes.is_empty() {
        return KEYED_TAGS
            .contains(&node.tag())
            .then(|| node.tag().to_owned());
    }
    Some(format!("{}.{}", node.tag(), classes.join(".")))
}

fn is_state_class(class: &str) -> bool {
    let lowered = class.to_ascii_lowercase();
    STATE_CLASSES.contains(&lowered.as_str())
        || lowered.starts_with("is-")
        || lowered.starts_with("has-")
}
