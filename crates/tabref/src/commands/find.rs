use serde_json::json;
use tabref_core::AppError;
use tabref_page::search_text;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::find_matches::find_matches;
use crate::view::find_view::find_lines;
use crate::view::outline_source::outline_input;
use crate::view::row_window::RowWindow;
use crate::view::view_options::ViewOptions;

const MAX_MATCHES: usize = 25;

/// Searches all of the page's text, shown or hidden, for `text`, and
/// prints each match with its section, the control in or near it and how
/// to show it when hidden. Frames the DOM capture did not read are searched
/// in the snapshot instead. Plain text never errors; no match is a result.
pub(crate) fn run(
    session: &mut Session,
    text: &str,
    role: Option<&str>,
) -> Result<Outcome, AppError> {
    let captured = session.capture()?;
    if role.is_some() {
        return Ok(snapshot_find(&captured.tree, &captured.info.url, text, role));
    }
    let input = outline_input(session, Some(&captured.tree))?;
    let (matches, total) = search_text(&input, text, MAX_MATCHES);
    if total == 0 {
        return Ok(snapshot_find(&captured.tree, &captured.info.url, text, None));
    }
    let options = ViewOptions::snap(&captured.info.url, false, RowWindow::default());
    let lines = find_lines(text, &matches, total, &options);
    Ok(Outcome::new(
        lines.join("\n"),
        json!({
            "snapshot_id": captured.snapshot_id,
            "url": captured.info.url,
            "count": total,
            "matches": lines.iter().skip(1).collect::<Vec<_>>(),
        }),
    ))
}

fn snapshot_find(tree: &tabref_core::AccessibilityNode, url: &str, text: &str, role: Option<&str>) -> Outcome {
    let matches = find_matches(tree, text, role);
    let shown: Vec<&String> = matches.iter().take(MAX_MATCHES).collect();
    let mut out = if matches.is_empty() {
        format!("0 matches for \"{text}\" on {url}")
    } else {
        format!("{} matches for \"{text}\"\n{}", matches.len(), shown.iter().map(|line| line.as_str()).collect::<Vec<_>>().join("\n"))
    };
    if matches.len() > MAX_MATCHES {
        out.push_str(&format!("\n… {} more; search a longer text or pass --role", matches.len() - MAX_MATCHES));
    }
    Outcome::new(out, json!({ "url": url, "count": matches.len(), "matches": shown }))
}
