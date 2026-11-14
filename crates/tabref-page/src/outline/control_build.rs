use super::control::Control;
use super::dom_node::DomNode;
use super::dom_tree::DomTree;
use super::element_kind::role_of;
use super::ref_label::RefLabel;
use super::select_option::SelectOption;
use super::text_runs::{squash, subtree_text};
use super::url_resolve::resolve_href;

const SHOWN_STATES: &[&str] = &["checked", "disabled", "expanded", "pressed", "selected"];

const TYPED_INPUTS: &[&str] = &[
    "date", "time", "datetime-local", "month", "week", "number", "range", "file", "password",
    "email", "color",
];

const AX_CONTROL_ROLES: &[&str] = &[
    "textfield", "radiobutton", "incrementor", "listbox", "menubutton", "treeitem", "colorwell",
    "disclosure",
];

const CONTROL_ROLES: &[&str] = &[
    "button", "link", "checkbox", "radio", "switch", "tab", "menuitem", "menuitemcheckbox",
    "menuitemradio", "option", "textbox", "searchbox", "combobox", "slider", "spinbutton",
];

/// The control the element at `index` is, named and described as the
/// accessibility snapshot has it when it has a ref. An element is a control
/// when it has a ref, is a link with an address, a form field, a button or
/// a `<summary>`, or has a control role.
pub(crate) fn control_of(tree: &DomTree, index: usize, label: Option<&RefLabel>) -> Option<Control> {
    let node = tree.node(index)?;
    let dom_role = dom_role(node);
    let label = label.filter(|label| label.clickable || CONTROL_ROLES.contains(&label.role.as_str()) || AX_CONTROL_ROLES.contains(&label.role.as_str()));
    if label.is_none() && dom_role.is_none() {
        return None;
    }
    let role = label
        .map(|label| label.role.clone())
        .or_else(|| dom_role.map(str::to_owned))
        .unwrap_or_default();
    let href = node
        .attr("href")
        .filter(|_| node.tag() == "A" || node.tag() == "AREA")
        .filter(|href| !href.trim_start().to_ascii_lowercase().starts_with("javascript:"))
        .map(|href| resolve_href(base_of(tree), href));
    let options = if node.tag() == "SELECT" {
        select_options(tree, index)
    } else {
        Vec::new()
    };
    let value = field_value(node, &options).or_else(|| label.and_then(|label| label.value.clone()));
    Some(Control {
        ref_text: label.map(|label| label.ref_text.clone()),
        role,
        name: control_name(tree, index, label),
        href,
        value: value.filter(|value| !value.is_empty()),
        states: control_states(tree, index, label),
        options,
    })
}

fn base_of(tree: &DomTree) -> &str {
    if tree.base_url.is_empty() {
        &tree.url
    } else {
        &tree.base_url
    }
}

fn dom_role(node: &DomNode) -> Option<&'static str> {
    if let Some(role) = role_of(node) {
        if let Some(known) = CONTROL_ROLES.iter().find(|known| **known == role) {
            return Some(known);
        }
    }
    match node.tag() {
        "A" | "AREA" if node.attr("href").is_some() => Some("link"),
        "BUTTON" => Some("button"),
        "SELECT" => Some("combobox"),
        "TEXTAREA" => Some("textfield"),
        "SUMMARY" => Some("disclosure"),
        "INPUT" => Some(input_role(node)),
        _ => None,
    }
}

fn input_role(node: &DomNode) -> &'static str {
    match input_type(node).as_str() {
        "checkbox" => "checkbox",
        "radio" => "radiobutton",
        "submit" | "button" | "reset" | "image" => "button",
        "range" => "slider",
        _ => "textfield",
    }
}

fn input_type(node: &DomNode) -> String {
    node.attr("type")
        .unwrap_or("text")
        .trim()
        .to_ascii_lowercase()
}

fn control_name(tree: &DomTree, index: usize, label: Option<&RefLabel>) -> String {
    if let Some(name) = label
        .and_then(|label| label.name.as_deref())
        .map(squash)
        .filter(|name| !name.is_empty())
    {
        return name;
    }
    let Some(node) = tree.node(index) else {
        return String::new();
    };
    let text = if node.tag() == "SELECT" || node.tag() == "TEXTAREA" {
        String::new()
    } else {
        subtree_text(tree, index)
    };
    [
        node.attr("aria-label").map(squash),
        Some(text),
        node.attr("title").map(squash),
        node.attr("placeholder").map(squash),
        image_alt(tree, index),
        node.attr("value")
            .filter(|_| input_role(node) == "button")
            .map(squash),
        node.attr("name").map(squash),
    ]
    .into_iter()
    .flatten()
    .find(|name| !name.is_empty())
    .unwrap_or_default()
}

fn image_alt(tree: &DomTree, index: usize) -> Option<String> {
    let node = tree.node(index)?;
    node.children.iter().find_map(|child| {
        let child_node = tree.node(*child)?;
        if child_node.tag() == "IMG" {
            return child_node.attr("alt").map(squash).filter(|alt| !alt.is_empty());
        }
        image_alt(tree, *child)
    })
}

fn field_value(node: &DomNode, options: &[SelectOption]) -> Option<String> {
    match node.tag() {
        "SELECT" => options
            .iter()
            .find(|option| option.selected)
            .map(|option| option.label.clone()),
        "TEXTAREA" => node.form.input_value.clone(),
        "INPUT" if input_role(node) == "textfield" || input_role(node) == "slider" => {
            node.form.input_value.clone()
        }
        _ => None,
    }
}

fn select_options(tree: &DomTree, index: usize) -> Vec<SelectOption> {
    let mut options = Vec::new();
    let mut pending: Vec<usize> = tree
        .node(index)
        .map(|node| node.children.iter().rev().copied().collect())
        .unwrap_or_default();
    while let Some(current) = pending.pop() {
        let Some(node) = tree.node(current) else {
            continue;
        };
        match node.tag() {
            "OPTION" => {
                let text = subtree_text(tree, current);
                let label = node.attr("label").map(squash).filter(|label| !label.is_empty()).unwrap_or(text);
                let value = node.attr("value").map(str::to_owned).unwrap_or_else(|| label.clone());
                options.push(SelectOption {
                    label,
                    value,
                    selected: node.form.option_selected,
                });
            }
            "OPTGROUP" => pending.extend(node.children.iter().rev()),
            _ => {}
        }
    }
    options
}

fn control_states(tree: &DomTree, index: usize, label: Option<&RefLabel>) -> Vec<String> {
    let mut states: Vec<String> = label
        .map(|label| {
            label
                .states
                .iter()
                .filter(|state| SHOWN_STATES.contains(&state.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let Some(node) = tree.node(index) else {
        return states;
    };
    let mut add = |state: &str| {
        if !states.iter().any(|known| known == state) {
            states.push(state.to_owned());
        }
    };
    if node.tag() == "INPUT" && node.form.checked {
        add("checked");
    }
    if node.attr("disabled").is_some() {
        add("disabled");
    }
    if node.attr("aria-expanded") == Some("false") || is_closed_summary(tree, index) {
        add("collapsed");
    }
    if node.tag() == "INPUT" {
        let kind = input_type(node);
        if TYPED_INPUTS.contains(&kind.as_str()) {
            add(&format!("type={kind}"));
        }
    }
    states.retain(|state| state != "expanded" || node.attr("aria-expanded") == Some("true"));
    states
}

fn is_closed_summary(tree: &DomTree, index: usize) -> bool {
    let Some(node) = tree.node(index) else {
        return false;
    };
    node.tag() == "SUMMARY"
        && node
            .parent
            .and_then(|parent| tree.node(parent))
            .is_some_and(|parent| parent.tag() == "DETAILS" && parent.attr("open").is_none())
}

#[cfg(test)]
#[path = "control_build_tests.rs"]
mod tests;
