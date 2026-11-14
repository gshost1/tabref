use super::block_kind::BlockKind;
use super::dom_node::DomNode;

const SKIPPED_TAGS: &[&str] = &[
    "HEAD", "SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "META", "LINK", "TITLE", "SVG", "CANVAS",
    "VIDEO", "AUDIO", "OBJECT", "EMBED", "SOURCE", "TRACK", "MAP", "IMG", "PICTURE", "HR", "WBR",
    "PARAM", "DATALIST",
];

const SECTIONING: &[&str] = &["ARTICLE", "ASIDE", "MAIN", "NAV", "SECTION"];

/// Whether the element and everything inside it stay out of the outline:
/// metadata, media, scripts, and what the page hides from assistive
/// technology.
pub(crate) fn is_skipped(node: &DomNode) -> bool {
    node.form.pseudo
        || SKIPPED_TAGS.contains(&node.tag())
        || node.attr("aria-hidden") == Some("true")
        || (node.tag() == "INPUT" && node.attr("type").is_some_and(|kind| kind.eq_ignore_ascii_case("hidden")))
}

/// The element's first ARIA role token, lower-cased.
pub(crate) fn role_of(node: &DomNode) -> Option<String> {
    node.attr("role")
        .and_then(|role| role.split_whitespace().next())
        .map(str::to_ascii_lowercase)
}

/// The block an element opens by what it means, before its layout is
/// looked at. `in_section` says whether a sectioning element holds it, which
/// stops `<header>` and `<footer>` from being page landmarks.
pub(crate) fn semantic_kind(node: &DomNode, in_section: bool) -> Option<BlockKind> {
    let role = role_of(node);
    if let Some(kind) = role.as_deref().and_then(|role| kind_of_role(role, node)) {
        return Some(kind);
    }
    let landmark = |name: &str| Some(BlockKind::Landmark(name.to_owned()));
    match node.tag() {
        "H1" | "H2" | "H3" | "H4" | "H5" | "H6" => {
            let level = node.tag()[1..].parse().unwrap_or(2);
            Some(BlockKind::Heading(level))
        }
        "P" | "FIGCAPTION" | "CAPTION" | "LEGEND" | "ADDRESS" => Some(BlockKind::Paragraph),
        "UL" | "OL" | "MENU" => Some(BlockKind::List),
        "LI" => Some(BlockKind::ListItem),
        "DL" => Some(BlockKind::List),
        "DT" => Some(BlockKind::Term),
        "DD" => Some(BlockKind::Description),
        "TABLE" if role.as_deref() != Some("presentation") && role.as_deref() != Some("none") => {
            Some(BlockKind::Table)
        }
        "TR" => Some(BlockKind::Row),
        "TD" => Some(BlockKind::Cell),
        "TH" => Some(BlockKind::HeaderCell),
        "BLOCKQUOTE" => Some(BlockKind::Quote),
        "PRE" => Some(BlockKind::Pre),
        "DETAILS" => Some(BlockKind::Details),
        "MAIN" => landmark("main"),
        "NAV" => landmark("nav"),
        "ASIDE" => landmark("aside"),
        "FORM" => landmark("form"),
        "SEARCH" => landmark("search"),
        "HEADER" if !in_section => landmark("header"),
        "FOOTER" if !in_section => landmark("footer"),
        "SECTION" if has_label(node) => landmark("region"),
        _ => None,
    }
}

fn kind_of_role(role: &str, node: &DomNode) -> Option<BlockKind> {
    let landmark = |name: &str| Some(BlockKind::Landmark(name.to_owned()));
    match role {
        "heading" => {
            let level = node
                .attr("aria-level")
                .and_then(|level| level.trim().parse().ok())
                .unwrap_or(2);
            Some(BlockKind::Heading(level))
        }
        "paragraph" => Some(BlockKind::Paragraph),
        "list" => Some(BlockKind::List),
        "listitem" => Some(BlockKind::ListItem),
        "table" | "grid" | "treegrid" => Some(BlockKind::Table),
        "row" => Some(BlockKind::Row),
        "cell" | "gridcell" => Some(BlockKind::Cell),
        "columnheader" | "rowheader" => Some(BlockKind::HeaderCell),
        "main" => landmark("main"),
        "navigation" => landmark("nav"),
        "banner" => landmark("header"),
        "contentinfo" => landmark("footer"),
        "complementary" => landmark("aside"),
        "form" => landmark("form"),
        "search" => landmark("search"),
        "region" if has_label(node) => landmark("region"),
        "dialog" | "alertdialog" => Some(BlockKind::Dialog),
        _ => None,
    }
}

fn has_label(node: &DomNode) -> bool {
    node.attr("aria-label")
        .is_some_and(|label| !label.trim().is_empty())
        || node.attr("aria-labelledby").is_some()
}

pub(crate) fn is_sectioning(node: &DomNode) -> bool {
    SECTIONING.contains(&node.tag())
}

/// Whether a computed `display` starts a box of its own rather than
/// sitting in the line around it.
pub(crate) fn is_block_display(display: &str) -> bool {
    !(display.is_empty()
        || display.starts_with("inline")
        || display == "contents"
        || display == "none"
        || display == "ruby"
        || display.starts_with("table-column"))
}

/// Whether the element is a dialog by markup: `role=dialog` or
/// `alertdialog`, an open `<dialog>`, or `aria-modal="true"`.
pub(crate) fn is_dialog_markup(node: &DomNode) -> bool {
    matches!(role_of(node).as_deref(), Some("dialog" | "alertdialog"))
        || (node.tag() == "DIALOG" && node.is_shown())
        || node.attr("aria-modal") == Some("true")
}

#[cfg(test)]
#[path = "element_kind_tests.rs"]
mod tests;
