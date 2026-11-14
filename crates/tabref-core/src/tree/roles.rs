/// Roles that always receive a ref: the controls an agent acts on.
pub const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "cell",
    "checkbox",
    "colorwell",
    "combobox",
    "incrementor",
    "link",
    "listbox",
    "menubutton",
    "menuitem",
    "option",
    "radiobutton",
    "slider",
    "switch",
    "tab",
    "textfield",
    "treeitem",
];

/// Every role a page snapshot emits.
pub const CANONICAL_ROLES: &[&str] = &[
    "alert",
    "alertdialog",
    "article",
    "banner",
    "button",
    "cell",
    "checkbox",
    "colorwell",
    "column",
    "combobox",
    "complementary",
    "contentinfo",
    "datefield",
    "definition",
    "dialog",
    "disclosure",
    "document",
    "form",
    "grid",
    "group",
    "heading",
    "image",
    "incrementor",
    "levelindicator",
    "link",
    "list",
    "listbox",
    "log",
    "main",
    "marquee",
    "math",
    "menu",
    "menubutton",
    "menuitem",
    "navigation",
    "note",
    "option",
    "outline",
    "paragraph",
    "progressbar",
    "radiobutton",
    "radiogroup",
    "region",
    "row",
    "scrollbar",
    "search",
    "separator",
    "slider",
    "statictext",
    "status",
    "switch",
    "tab",
    "table",
    "tablist",
    "tabpanel",
    "term",
    "textfield",
    "timefield",
    "timer",
    "toolbar",
    "tooltip",
    "treeitem",
    "webarea",
];

pub fn is_interactive_role(role: &str) -> bool {
    INTERACTIVE_ROLES.contains(&role)
}

pub fn is_canonical_role(role: &str) -> bool {
    CANONICAL_ROLES.contains(&role)
}

#[cfg(test)]
#[path = "roles_tests.rs"]
mod tests;
