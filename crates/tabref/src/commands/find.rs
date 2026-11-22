use std::collections::HashSet;

use serde_json::{Value, json};
use tabref_core::{AccessibilityNode, AppError};
use tabref_page::{OutlineInput, TextMatch, squash};

use super::find_room::{room_note, scroll_room};
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::find_matches::{find_matches, visit_matches};
use crate::view::find_view::find_lines_with_ax;
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
/// In the daemon, the last capture stands for the page when the page has
/// provably not changed since.
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
/// the `find` step of `do`. The step keeps every matching line in JSON
/// and applies the display limit only to its text.
pub(crate) fn search(
    session: &mut Session,
    text: &str,
    role: Option<&str>,
    keep: usize,
    all_matches: bool,
) -> Result<Found, AppError> {
    let captured = session.current_capture()?;
    let url = captured.info.url.clone();
    let cap = if all_matches { usize::MAX } else { keep };
    let mut found = if role.is_some() {
        snapshot_find(&captured.tree, &url, text, role, cap)
    } else {
        let input = outline_input(session, Some(&captured))?;
        let (matches, dom_total, ax, ax_total) = merged_matches(&input, &captured.tree, text, cap);
        if dom_total == 0 {
            snapshot_result(&url, text, ax_total, ax)
        } else {
            let options = ViewOptions::snap(&url, false, RowWindow::default());
            let total = dom_total + ax_total;
            let mut lines = find_lines_with_ax(text, &matches, &ax, total, &options);
            let head = lines.remove(0);
            let more = (total > matches.len() + ax.len())
                .then(|| lines.pop())
                .flatten();
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
    found.lines.truncate(keep);
    if found.count == 0 {
        found.note = scroll_room(session).map(|room| room_note(&room));
        if let Some(note) = &found.note {
            found.data["note"] = json!(note);
        }
    }
    Ok(found)
}

fn merged_matches(
    input: &OutlineInput,
    tree: &AccessibilityNode,
    needle: &str,
    cap: usize,
) -> (Vec<TextMatch>, usize, Vec<String>, usize) {
    let mut covered = HashSet::new();
    let (dom, dom_total) = TextMatch::search_with(input, needle, cap, |reference, source| {
        covered.insert((reference.map(str::to_owned), squash(source).to_lowercase()));
    });
    let mut ax = Vec::new();
    let mut ax_total = 0;
    let room = cap.saturating_sub(dom.len());
    visit_matches(tree, needle, None, |reference, sources, render| {
        let reference = reference.map(str::to_owned);
        if sources
            .iter()
            .all(|source| covered.contains(&(reference.clone(), squash(source).to_lowercase())))
        {
            return;
        }
        ax_total += 1;
        if ax.len() < room {
            ax.push(render());
        }
    });
    (dom, dom_total, ax, ax_total)
}

fn snapshot_find(
    tree: &AccessibilityNode,
    url: &str,
    text: &str,
    role: Option<&str>,
    keep: usize,
) -> Found {
    let matches = find_matches(tree, text, role);
    let count = matches.len();
    snapshot_result(url, text, count, matches.into_iter().take(keep).collect())
}

fn snapshot_result(url: &str, text: &str, count: usize, shown: Vec<String>) -> Found {
    let more = (count > shown.len()).then(|| {
        format!(
            "… {} more; search a longer text or pass --role",
            count - shown.len()
        )
    });
    let head = if count == 0 {
        format!("0 matches for \"{text}\" on {url}")
    } else {
        format!("{count} matches for \"{text}\"")
    };
    Found {
        count,
        head,
        data: json!({ "url": url, "count": count, "matches": shown }),
        lines: shown,
        more,
        note: None,
    }
}

#[cfg(test)]
#[path = "find_tests.rs"]
mod tests;
