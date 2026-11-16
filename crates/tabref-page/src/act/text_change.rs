use std::fmt;

const CONTEXT_CHARS: usize = 40;
const LINE_CHARS: usize = 100;
const ADDED_CHARS: usize = 400;
const MIDDLE_CHARS: usize = 60;
const BRIEF_ROWS_CHARS: usize = 80;
const ROWS_LABEL: &str = "now starting:";
const ROWS_SHOWN: usize = 5;
const ABOVE_CHARS: usize = 40;

/// One line of a page's rendered text that an action changed: rewritten in
/// place, added or removed; or a run of lines the action put in a new
/// order (a sorted list drawn again): the line above the run, the run's
/// lines in their new order, and whether a table drew them as rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextChange {
    Changed {
        before: String,
        after: String,
    },
    Added(String),
    Removed(String),
    Reordered {
        above: Option<String>,
        run: Vec<String>,
        table: bool,
    },
}

impl TextChange {
    /// The change as a `do` step's line carries it: as printed, with an
    /// added line cut to 100 characters like a removed one.
    pub fn brief(&self) -> String {
        match self {
            Self::Added(line) => format!("+ {}", cut(line, LINE_CHARS)),
            Self::Reordered { above, run, table } => brief_rows(&run_head(above, run, *table), run),
            other => other.to_string(),
        }
    }
}

impl fmt::Display for TextChange {
    /// `▲ {3 -> 4} ▼ Books set in winter` for a rewritten line, keeping the
    /// words around the change; `+ line` otherwise, whole up to 400
    /// characters so a validation message keeps its list, and `- line` cut
    /// to 100. A cut ends after a whole word. A reorder prints its first
    /// rows, one per indented line, and how to see the rest.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reordered { above, run, table } => {
                formatter.write_str(&rows_block(&run_head(above, run, *table), run))
            }
            Self::Changed { before, after } => formatter.write_str(&inline_change(before, after)),
            Self::Added(line) => write!(formatter, "+ {}", cut(line, ADDED_CHARS)),
            Self::Removed(line) => write!(formatter, "- {}", cut(line, LINE_CHARS)),
        }
    }
}

/// `reordered 8 rows below "Code Weight", now starting:`; `lines` when no
/// table drew them, and no `below` when the run starts the page.
fn run_head(above: &Option<String>, run: &[String], table: bool) -> String {
    let noun = if table { "rows" } else { "lines" };
    let below = above.as_ref().map_or_else(String::new, |line| {
        format!(" below \"{}\"", cut(line, ABOVE_CHARS))
    });
    format!("reordered {} {noun}{below}, {ROWS_LABEL}", run.len())
}

fn rows_block(head: &str, first: &[String]) -> String {
    let count = first.len();
    let mut block = head.to_owned();
    let shown = count.min(ROWS_SHOWN);
    for row in &first[..shown] {
        block.push_str("\n  ");
        block.push_str(&cut(row, LINE_CHARS));
    }
    if count > shown {
        block.push_str(&format!("\n  (+{} more: tabref snap)", count - shown));
    }
    block
}

fn brief_rows(head: &str, first: &[String]) -> String {
    let count = first.len();
    let mut rows = String::new();
    let mut shown = 0;
    for row in first {
        let row = cut(row, BRIEF_ROWS_CHARS);
        let joined = rows.chars().count() + row.chars().count() + 3;
        if shown > 0 && joined > BRIEF_ROWS_CHARS {
            break;
        }
        if shown > 0 {
            rows.push_str(" | ");
        }
        rows.push_str(&row);
        shown += 1;
    }
    let more = if count > shown { " …" } else { "" };
    format!("{head} {rows}{more}")
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
        _ if !removed.contains(' ') && !added.contains(' ') => in_word(&removed, &added),
        _ => format!("{{{removed} -> {added}}}"),
    });
    let rest = old[old.len() - suffix..].join(" ");
    if !rest.is_empty() {
        line.push(' ');
        line.push_str(&cut(&rest, CONTEXT_CHARS));
    }
    line
}

/// One changed word shown by the characters that changed, as `▲{3 -> 4}`
/// for a counter glued to its button; a word that changed whole stays
/// whole.
fn in_word(removed: &str, added: &str) -> String {
    let old: Vec<char> = removed.chars().collect();
    let new: Vec<char> = added.chars().collect();
    let mut prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    while prefix > 0 && splits_number(&old, prefix) {
        prefix -= 1;
    }
    let room = old.len().min(new.len()) - prefix;
    let mut suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    while suffix > 0
        && (splits_number(&old, old.len() - suffix) || splits_number(&new, new.len() - suffix))
    {
        suffix -= 1;
    }
    if prefix + suffix == 0 {
        return format!("{{{removed} -> {added}}}");
    }
    let text = |chars: &[char]| chars.iter().collect::<String>();
    let (gone, came) = (
        text(&old[prefix..old.len() - suffix]),
        text(&new[prefix..new.len() - suffix]),
    );
    let middle = match (gone.is_empty(), came.is_empty()) {
        (true, _) => format!("{{-> {came}}}"),
        (_, true) => format!("{{{gone} ->}}"),
        _ => format!("{{{gone} -> {came}}}"),
    };
    format!(
        "{}{middle}{}",
        text(&old[..prefix]),
        text(&old[old.len() - suffix..])
    )
}

/// Whether a cut before `chars[at]` falls between two digits of a number.
fn splits_number(chars: &[char], at: usize) -> bool {
    at > 0 && chars.get(at).is_some_and(char::is_ascii_digit) && chars[at - 1].is_ascii_digit()
}

/// `text` cut to about `limit` characters, ending in `…` when cut: after
/// the last whole word that fits, or mid-word when the first half of the
/// room holds no space.
pub(crate) fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let kept: String = text.chars().take(limit).collect();
    let next_is_space = text.chars().nth(limit).is_some_and(char::is_whitespace);
    if next_is_space {
        return format!("{} …", kept.trim_end());
    }
    match kept.rfind(' ') {
        Some(space) if kept[..space].chars().count() >= limit / 2 => {
            format!("{} …", kept[..space].trim_end())
        }
        _ => format!("{kept}…"),
    }
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
