use std::fmt;

const CONTEXT_CHARS: usize = 40;
const LINE_CHARS: usize = 100;
const MIDDLE_CHARS: usize = 60;

/// One line of a page's rendered text that an action changed: rewritten in
/// place, added or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextChange {
    Changed { before: String, after: String },
    Added(String),
    Removed(String),
}

impl fmt::Display for TextChange {
    /// `▲ {3 -> 4} ▼ Books set in winter` for a rewritten line, keeping the
    /// words around the change; `+ line` and `- line` otherwise, cut to 100
    /// characters.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changed { before, after } => formatter.write_str(&inline_change(before, after)),
            Self::Added(line) => write!(formatter, "+ {}", cut(line, LINE_CHARS)),
            Self::Removed(line) => write!(formatter, "- {}", cut(line, LINE_CHARS)),
        }
    }
}

fn inline_change(before: &str, after: &str) -> String {
    let old: Vec<&str> = before.split(' ').collect();
    let new: Vec<&str> = after.split(' ').collect();
    let prefix = old
        .iter()
        .zip(&new)
        .take_while(|(left, right)| left == right)
        .count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(left, right)| left == right)
        .count();
    let removed = old[prefix..old.len() - suffix].join(" ");
    let added = new[prefix..new.len() - suffix].join(" ");
    if removed.chars().count() > MIDDLE_CHARS || added.chars().count() > MIDDLE_CHARS {
        return format!(
            "{} -> {}",
            cut(before, LINE_CHARS / 2),
            cut(after, LINE_CHARS / 2)
        );
    }
    let mut line = String::new();
    let head = old[..prefix].join(" ");
    if !head.is_empty() {
        line.push_str(&tail(&head, CONTEXT_CHARS));
        line.push(' ');
    }
    line.push_str(&match (removed.is_empty(), added.is_empty()) {
        (true, _) => format!("{{-> {added}}}"),
        (_, true) => format!("{{{removed} ->}}"),
        _ => format!("{{{removed} -> {added}}}"),
    });
    let rest = old[old.len() - suffix..].join(" ");
    if !rest.is_empty() {
        line.push(' ');
        line.push_str(&cut(&rest, CONTEXT_CHARS));
    }
    line
}

/// `text` cut to `limit` characters, ending in `…` when cut.
pub(crate) fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut kept: String = text.chars().take(limit).collect();
    kept.push('…');
    kept
}

fn tail(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_owned();
    }
    let kept: String = text.chars().skip(count - limit).collect();
    format!("…{kept}")
}

#[cfg(test)]
#[path = "text_change_tests.rs"]
mod tests;
