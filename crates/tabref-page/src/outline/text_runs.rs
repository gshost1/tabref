use super::dom_tree::DomTree;
use super::node_kind::NodeKind;

/// `text` with every run of whitespace made one space, keeping one space at
/// either end when the text had whitespace there, so runs joined later
/// keep the word breaks the page had.
pub fn collapse_run(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return if text.is_empty() { String::new() } else { " ".to_owned() };
    }
    let mut out = String::with_capacity(text.len());
    if text.starts_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(&words.join(" "));
    if text.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    out
}

/// `text` on one line with single spaces and no space at either end.
pub fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text inside the node at `index`, shown or not, on one line. Script
/// and style text is left out.
pub(crate) fn subtree_text(tree: &DomTree, index: usize) -> String {
    let mut out = String::new();
    let mut pending = vec![index];
    let mut seen = 0usize;
    while let Some(current) = pending.pop() {
        seen += 1;
        if seen > 20_000 {
            break;
        }
        let Some(node) = tree.node(current) else {
            continue;
        };
        match &node.kind {
            NodeKind::Text(text) => {
                out.push_str(text);
                out.push(' ');
            }
            NodeKind::Element(tag) if matches!(tag.as_str(), "SCRIPT" | "STYLE" | "TEMPLATE") => {}
            _ => pending.extend(node.children.iter().rev()),
        }
    }
    squash(&out)
}

/// `text` cut to at most `limit` characters, ending in `…` when cut.
pub fn cut_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut kept: String = text.chars().take(limit.saturating_sub(1)).collect();
    kept.push('…');
    kept
}

#[cfg(test)]
#[path = "text_runs_tests.rs"]
mod tests;
