use std::collections::BTreeSet;

use super::section_cut::SectionCut;
use super::view_line::ViewLine;
use super::view_route::ViewRoute;

const MORE_ROOM: usize = 100;
const HEADING_CHARS: usize = 100;

/// The headings past a view's cut, as plain lines that fit in `room`
/// characters: `## 8. Content (172 lines, --from 552)`, with no refs or
/// links, so the rest of the page stays countable and reachable at little
/// cost. Every heading of the shallowest levels that fit is listed, so each
/// top-level section is one `--from` or `read --section` away, and the room
/// left holds the first headings of the next level. A last line says how
/// many were left out and how to reach them.
pub(crate) struct CutOutline {
    pub(crate) lines: Vec<ViewLine>,
    pub(crate) sections: SectionCut,
}

struct Entry {
    level: u8,
    number: usize,
    line: ViewLine,
}

impl CutOutline {
    /// The characters the outline costs once printed.
    pub(crate) fn cost(&self) -> usize {
        self.lines.iter().map(ViewLine::cost).sum()
    }
}

/// The outline of the headings in `lines` from index `from` on.
pub(crate) fn cut_outline(
    lines: &[ViewLine],
    from: usize,
    room: usize,
    route: &ViewRoute,
    full: bool,
) -> CutOutline {
    let entries = entries(lines, from);
    let mut sections = SectionCut {
        count: entries.len(),
        listed: 0,
        first_heading: lines
            .iter()
            .skip(from)
            .find(|line| line.heading.is_some())
            .map(ViewLine::heading_text),
    };
    let chosen = choose(&entries, room);
    sections.listed = chosen.len();
    let mut printed: Vec<ViewLine> = chosen.iter().map(|&at| entries[at].line.clone()).collect();
    let first_left = (0..entries.len()).find(|at| !chosen.contains(at));
    if let Some(left) = first_left {
        printed.push(ViewLine::new(
            0,
            format!(
                "… {} more headings; tabref find <text> or {}",
                entries.len() - chosen.len(),
                route.starting_at(entries[left].number, full)
            ),
        ));
    }
    CutOutline {
        lines: printed,
        sections,
    }
}

fn entries(lines: &[ViewLine], from: usize) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (position, line) in lines.iter().enumerate().skip(from) {
        let Some(level) = line.heading else {
            continue;
        };
        let held = lines[position + 1..]
            .iter()
            .take_while(|next| next.heading.is_none_or(|next| next > level))
            .count();
        let mut text: String = line.heading_text().chars().take(HEADING_CHARS).collect();
        if text.chars().count() < line.heading_text().chars().count() {
            text.push('…');
        }
        let hashes = "#".repeat(usize::from(level.clamp(1, 6)));
        let mut printed = ViewLine::new(
            0,
            format!("{hashes} {text} ({held} lines, --from {})", line.number),
        );
        printed.number = line.number;
        printed.heading = Some(level);
        printed.plain = Some(line.heading_text());
        entries.push(Entry {
            level,
            number: line.number,
            line: printed,
        });
    }
    entries
}

/// The indexes of the entries to list, in page order: those of every level
/// up to the deepest one whose headings all fit, then as many of the next
/// level's as the room still holds, in order.
fn choose(entries: &[Entry], room: usize) -> Vec<usize> {
    let levels: BTreeSet<u8> = entries.iter().map(|entry| entry.level).collect();
    let cost_upto = |deepest: u8| -> usize {
        entries
            .iter()
            .filter(|entry| entry.level <= deepest)
            .map(|entry| entry.line.cost())
            .sum()
    };
    let mut whole = None;
    for &level in &levels {
        let all = levels.iter().all(|&other| other <= level);
        let cost = cost_upto(level) + if all { 0 } else { MORE_ROOM };
        if cost > room {
            break;
        }
        whole = Some(level);
    }
    let mut chosen: Vec<usize> = match whole {
        Some(deepest) => (0..entries.len())
            .filter(|&at| entries[at].level <= deepest)
            .collect(),
        None => Vec::new(),
    };
    let next = levels
        .iter()
        .copied()
        .find(|&level| whole.is_none_or(|deepest| level > deepest));
    if let Some(next) = next {
        let mut spent = MORE_ROOM
            + chosen
                .iter()
                .map(|&at| entries[at].line.cost())
                .sum::<usize>();
        for (at, entry) in entries.iter().enumerate() {
            if entry.level != next {
                continue;
            }
            if spent + entry.line.cost() > room {
                break;
            }
            spent += entry.line.cost();
            chosen.push(at);
        }
        chosen.sort_unstable();
    }
    chosen
}
