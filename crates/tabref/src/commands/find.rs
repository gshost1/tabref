use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use tabref_core::{AccessibilityNode, AppError};
use tabref_page::{Control, DomTree, OutlineInput, TextMatch, squash};

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
        let (matches, total, ax_only) = merged_matches(&input, &captured.tree, text, cap);
        if ax_only {
            snapshot_result(
                &url,
                text,
                total,
                matches.into_iter().map(|hit| hit.line).collect(),
            )
        } else {
            let options = ViewOptions::snap(&url, false, RowWindow::default());
            let mut lines = find_lines_with_ax(text, &matches, &[], total, &options);
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
) -> (Vec<TextMatch>, usize, bool) {
    let trees: Vec<&DomTree> = input
        .trees
        .iter()
        .chain(input.frames.values().flatten())
        .collect();
    let mut covered: HashMap<&str, HashSet<i64>> = HashMap::new();
    let mut sources = HashSet::new();
    let mut nearest = HashMap::new();
    let folded = squash(needle).to_lowercase();
    let (dom, total) = TextMatch::search_located(input, needle, cap, |tree, index, source| {
        let source = source.to_lowercase();
        if folded.is_empty() || !source.contains(&folded) {
            return;
        }
        covered
            .entry(tree.target_id.as_str())
            .or_default()
            .insert(tree.nodes[index].backend_id);
        let reference = nearest
            .entry((tree.target_id.as_str(), tree.nodes[index].backend_id))
            .or_insert_with(|| {
                let mut at = Some(index);
                while let Some(index) = at {
                    let node = &tree.nodes[index];
                    if let Some(label) = input.refs.get(&(tree.target_id.clone(), node.backend_id))
                    {
                        return Some(label.ref_text.clone());
                    }
                    at = node.parent;
                }
                None
            });
        sources.insert((reference.clone(), source));
    });
    if total == 0 {
        let ax = find_matches(tree, needle, None);
        let total = ax.len();
        return (
            ax.into_iter()
                .take(cap)
                .map(|line| TextMatch {
                    section: Vec::new(),
                    line,
                    control: None,
                    hidden: false,
                    expander: None,
                })
                .collect(),
            total,
            true,
        );
    }
    let reverse: HashMap<&str, &(String, i64)> = input
        .refs
        .iter()
        .map(|(key, label)| (label.ref_text.as_str(), key))
        .collect();
    let mut positions = HashMap::new();
    let mut offset = 0;
    for tree in &trees {
        for (index, node) in tree.nodes.iter().enumerate() {
            positions.insert((tree.target_id.clone(), node.backend_id), offset + index);
        }
        offset += tree.nodes.len();
    }
    expand_coverage(&trees, &mut covered);
    let mut ordered: Vec<(usize, TextMatch)> = dom
        .into_iter()
        .map(|(target, backend, hit)| {
            (
                positions
                    .get(&(target, backend))
                    .copied()
                    .unwrap_or(usize::MAX),
                hit,
            )
        })
        .collect();
    ordered.sort_by_key(|(position, _)| *position);
    let mut ax_total = 0;
    let mut sections = None;
    visit_matches(tree, needle, None, |reference, texts, _| {
        let key = reference.and_then(|reference| reverse.get(reference).copied());
        let in_dom = key.is_some_and(|(target, backend)| {
            covered
                .get(target.as_str())
                .is_some_and(|nodes| nodes.contains(backend))
        });
        if in_dom
            || texts.iter().all(|source| {
                sources.contains(&(reference.map(str::to_owned), squash(source).to_lowercase()))
            })
        {
            return;
        }
        ax_total += 1;
        let located = key.and_then(|key| positions.get(key));
        let position = located.copied().unwrap_or(usize::MAX);
        if ordered.len() == cap
            && (cap == 0 || ordered.last().is_some_and(|(last, _)| position >= *last))
        {
            return;
        }
        let control = key
            .and_then(|key| input.refs.get(key))
            .map(|label| Control {
                ref_text: Some(label.ref_text.clone()),
                role: label.role.clone(),
                name: label.name.clone().unwrap_or_default(),
                value: label.value.clone(),
                states: label.states.clone(),
                ..Control::default()
            });
        let section = located.map_or_else(Vec::new, |position| {
            let paths = sections.get_or_insert_with(|| section_paths(&trees));
            let past = paths.partition_point(|(start, _)| start <= position);
            paths[..past]
                .last()
                .map_or_else(Vec::new, |(_, path)| path.clone())
        });
        ordered.push((
            position,
            TextMatch {
                section,
                line: texts.join(" "),
                control,
                hidden: false,
                expander: None,
            },
        ));
        ordered.sort_by_key(|(position, _)| *position);
        ordered.truncate(cap);
    });
    ordered.sort_by_key(|(position, _)| *position);
    (
        ordered.into_iter().take(cap).map(|(_, hit)| hit).collect(),
        total + ax_total,
        false,
    )
}

fn section_paths(trees: &[&DomTree]) -> Vec<(usize, Vec<String>)> {
    let mut sections = Vec::new();
    let mut offset = 0;
    for tree in trees {
        let mut path: Vec<(u8, String)> = Vec::new();
        sections.push((offset, Vec::new()));
        for (index, node) in tree.nodes.iter().enumerate() {
            if let Some(level) = node
                .tag()
                .strip_prefix('H')
                .and_then(|level| level.parse::<u8>().ok())
                .filter(|level| (1..=6).contains(level))
            {
                path.retain(|(known, _)| *known < level);
                path.push((level, tabref_page::subtree_text(tree, index)));
                sections.push((
                    offset + index + 1,
                    path.iter().map(|(_, text)| text.clone()).collect(),
                ));
            }
        }
        offset += tree.nodes.len();
    }
    sections
}

fn expand_coverage(trees: &[&DomTree], covered: &mut HashMap<&str, HashSet<i64>>) {
    if covered.is_empty() {
        return;
    }
    for tree in trees {
        let Some(covered) = covered.get_mut(tree.target_id.as_str()) else {
            continue;
        };
        let mut inside = vec![false; tree.nodes.len()];
        let mut labels = HashSet::new();
        let mut ids = HashSet::new();
        for (index, node) in tree.nodes.iter().enumerate() {
            inside[index] = covered.contains(&node.backend_id)
                || node.parent.is_some_and(|parent| inside[parent]);
            if inside[index] {
                if let Some(id) = node.attr("for") {
                    labels.insert(id);
                }
                if let Some(id) = node.attr("id") {
                    ids.insert(id);
                }
            }
        }
        for (index, node) in tree.nodes.iter().enumerate() {
            if inside[index]
                || node.attr("id").is_some_and(|id| labels.contains(id))
                || node
                    .attr("aria-labelledby")
                    .is_some_and(|names| names.split_whitespace().any(|id| ids.contains(id)))
            {
                covered.insert(node.backend_id);
            }
        }
    }
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
