use tabref_page::{TextMatch, cut_chars};

use super::control_text::control_text;
use super::view_options::ViewOptions;

const SECTION_DEPTH: usize = 2;
const HEADING_LIMIT: usize = 60;

/// The lines of a find result: a count line, then one line per match with
/// the headings it sits under, its text, the control in or near it, and,
/// when the page hides it, the control that shows it.
pub(crate) fn find_lines(
    needle: &str,
    matches: &[TextMatch],
    total: usize,
    options: &ViewOptions,
) -> Vec<String> {
    let hidden = matches.iter().filter(|found| found.hidden).count();
    let noun = if total == 1 { "match" } else { "matches" };
    let mut head = format!("{total} {noun} for \"{needle}\"");
    if hidden > 0 {
        head.push_str(&format!(" ({hidden} hidden)"));
    }
    let mut lines = vec![head];
    for found in matches {
        lines.push(match_line(found, options));
    }
    if total > matches.len() {
        lines.push(format!(
            "… {} more; search a longer text, or read --section \"<heading>\"",
            total - matches.len()
        ));
    }
    lines
}

fn match_line(found: &TextMatch, options: &ViewOptions) -> String {
    let mut line = String::from("- ");
    let path: Vec<String> = found
        .section
        .iter()
        .rev()
        .take(SECTION_DEPTH)
        .rev()
        .map(|heading| cut_chars(heading, HEADING_LIMIT))
        .collect();
    if !path.is_empty() {
        line.push_str(&path.join(" > "));
        line.push_str(" | ");
    }
    line.push_str(&found.line);
    if let Some(control) = &found.control {
        let token = control_text(control, options, true);
        if !found.line.contains(&token) {
            line.push(' ');
            line.push_str(&token);
        }
    }
    if found.hidden {
        match &found.expander {
            Some(expander) => {
                line.push_str(" (hidden; expand ");
                line.push_str(&control_text(expander, options, false));
                line.push(')');
            }
            None => line.push_str(" (hidden)"),
        }
    }
    line
}
