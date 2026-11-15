//! Keeps page text from imitating the view's own markup: a bracket in page
//! text could forge a control token (`[e3 button Cancel order]`), and a
//! page line could open like a heading, a dialog line or the footer.

use tabref_page::squash;

const RESERVED_STARTS: &[&str] = &[
    "#", "--", "dialog", "frame ", "frames:", "page ", "table ", "…", "- ", "> ", "| ", "\\",
];

/// Page text with `[` and `]` escaped, on one line.
pub(crate) fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            '\n' | '\r' => out.push(' '),
            other => out.push(other),
        }
    }
    out
}

/// A name or option inside a token: escaped text, `|` escaped too, on one
/// line with single spaces.
pub(crate) fn escape_in_token(text: &str) -> String {
    escape_text(&squash(text)).replace('|', "\\|")
}

/// An address as the view prints it: characters that could end the token
/// or the line are percent-encoded.
pub(crate) fn escape_href(href: &str) -> String {
    let mut out = String::with_capacity(href.len());
    for character in href.chars() {
        match character {
            ' ' => out.push_str("%20"),
            '"' => out.push_str("%22"),
            '[' => out.push_str("%5B"),
            ']' => out.push_str("%5D"),
            '|' => out.push_str("%7C"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            '\t' => out.push_str("%09"),
            other => out.push(other),
        }
    }
    out
}

/// A line of page content with a backslash before it when it would open
/// like the view's own lines (`# `, `--`, `dialog`, `frames:`, `(2/5) `).
pub(crate) fn escape_line_start(line: String) -> String {
    let counted = line
        .strip_prefix('(')
        .and_then(|rest| rest.split_once(") "))
        .is_some_and(|(count, _)| {
            count
                .split_once('/')
                .is_some_and(|(a, b)| is_number(a) && is_number(b))
        });
    let items = line
        .split_once(" items:")
        .is_some_and(|(count, _)| is_number(count));
    if counted || items || RESERVED_STARTS.iter().any(|start| line.starts_with(start)) {
        return format!("\\{line}");
    }
    line
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
#[path = "escape_tests.rs"]
mod tests;
