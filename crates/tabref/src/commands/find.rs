use serde_json::{Value, json};
use tabref_core::AppError;
use tabref_page::search_text;

use super::find_room::{room_note, scroll_room};
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::find_matches::find_matches;
use crate::view::find_view::find_lines;
use crate::view::outline_source::outline_input;
use crate::view::row_window::RowWindow;
use crate::view::view_options::ViewOptions;

const MAX_MATCHES: usize = 20;

/// A find's result: how many matched, its first line, the line per match
/// as far as the cap goes, the line `tabref find` prints for the rest,
/// and, with no match on a page that loads more as it scrolls, the note
/// that says how to load it.
pub(crate) struct Found {
    pub count: usize,
    pub head: String,
    pub lines: Vec<String>,
    pub more: Option<String>,
    pub note: Option<String>,
    pub data: Value,
}

/// Searches all of the page's text, shown or hidden, for `text`, and
/// prints each match with its section, the control in or near it and how
/// to show it when hidden. Frames the DOM capture did not read are searched
/// in the snapshot instead. Plain text never errors; no match is a result.
pub(crate) fn run(
    session: &mut Session,
    text: &str,
    role: Option<&str>,
) -> Result<Outcome, AppError> {
    let found = search(session, text, role, MAX_MATCHES, false)?;
    let mut head = found.head;
    if let Some(note) = &found.note {
        head.push_str("; ");
        head.push_str(note);
    }
    let mut lines = vec![head];
    lines.extend(found.lines);
    lines.extend(found.more);
    Ok(Outcome::new(lines.join("\n"), found.data))
}

/// [`run`]'s search, keeping `keep` match lines, for `tabref find` and
/// the `find` step of `do`. With `tree_matches`, the JSON `matches` are
/// the snapshot's matching lines, as `do --json` has always printed them.
pub(crate) fn search(
    session: &mut Session,
    text: &str,
    role: Option<&str>,
    keep: usize,
    tree_matches: bool,
) -> Result<Found, AppError> {
    let captured = session.capture()?;
    let url = captured.info.url.clone();
    let mut found = if role.is_some() {
        snapshot_find(&captured.tree, &url, text, role, keep)
    } else {
        let input = outline_input(session, Some(&captured))?;
        let (matches, total) = search_text(&input, text, keep);
        if total == 0 {
            snapshot_find(&captured.tree, &url, text, None, keep)
        } else {
            let options = ViewOptions::snap(&url, false, RowWindow::default());
            let mut lines = find_lines(text, &matches, total, &options);
            let head = lines.remove(0);
            let more = (total > matches.len()).then(|| lines.pop()).flatten();
            let mut listed = lines.clone();
            listed.extend(more.clone());
            Found {
                count: total,
                head,
                data: json!({
                    "snapshot_id": captured.snapshot_id,
                    "url": url,
                    "count": total,
                    "matches": listed,
                }),
                lines,
                more,
                note: None,
            }
        }
    };
    if tree_matches {
        found.data["matches"] = json!(find_matches(&captured.tree, text, role));
    }
    if found.count == 0 {
        found.note = scroll_room(session).map(|room| room_note(&room));
        if let Some(note) = &found.note {
            found.data["note"] = json!(note);
        }
    }
    Ok(found)
}

fn snapshot_find(
    tree: &tabref_core::AccessibilityNode,
    url: &str,
    text: &str,
    role: Option<&str>,
    keep: usize,
) -> Found {
    let matches = find_matches(tree, text, role);
    let shown: Vec<String> = matches.iter().take(keep).cloned().collect();
    let more = (matches.len() > keep).then(|| {
        format!(
            "… {} more; search a longer text or pass --role",
            matches.len() - keep
        )
    });
    let head = if matches.is_empty() {
        format!("0 matches for \"{text}\" on {url}")
    } else {
        format!("{} matches for \"{text}\"", matches.len())
    };
    Found {
        count: matches.len(),
        head,
        data: json!({ "url": url, "count": matches.len(), "matches": shown }),
        lines: shown,
        more,
        note: None,
    }
}
