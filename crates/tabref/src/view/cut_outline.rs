use std::collections::BTreeSet;

use super::escape::escape_text;
use super::section_cut::SectionCut;
use super::view_line::ViewLine;
use super::view_route::ViewRoute;

const MORE_ROOM: usize = 100;
const HEADING_CHARS: usize = 100;
const BODY_CHARS: usize = 160;

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
    pub(crate) stub_cost: usize,
}

struct Entry {
    level: u8,
    number: usize,
    line: ViewLine,
    inline: Option<ViewLine>,
    body_cost: usize,
}

impl CutOutline {
    /// The characters the outline costs once printed.
    #[cfg(test)]
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
    let boundary = route
        .to
        .filter(|&to| lines.last().is_some_and(|line| line.number >= to));
    let entries = entries(lines, from, boundary);
    let mut sections = SectionCut {
        count: entries.len(),
        listed: 0,
    };
    let (chosen, listing) = choose(&entries, room);
    sections.listed = chosen.len();
    let mut printed: Vec<ViewLine> = chosen.iter().map(|&at| entries[at].line.clone()).collect();
    let mut listed = vec![false; entries.len()];
    for &at in &chosen {
        listed[at] = true;
    }
    let left_out = |at: &usize| !listed[*at];
    let first_left = (0..entries.len())
        .filter(left_out)
        .find(|&at| listing.is_none_or(|level| entries[at].level <= level))
        .or_else(|| (0..entries.len()).find(left_out));
    if let Some(left) = first_left {
        let more = ViewLine::new(
            0,
            format!(
                "… {} more headings; tabref find <text> or {}",
                entries.len() - chosen.len(),
                route.starting_at(entries[left].number, full)
            ),
        );
        if printed.iter().map(ViewLine::cost).sum::<usize>() + more.cost() <= room {
            printed.push(more);
        }
    }
    let mut spent: usize = printed.iter().map(ViewLine::cost).sum();
    let stub_cost = spent;
    let mut body_spent = 0;
    let mut priority: Vec<usize> = (0..chosen.len()).collect();
    priority.sort_by_key(|&at| (entries[chosen[at]].level, chosen[at]));
    for at in priority {
        let entry = &entries[chosen[at]];
        let Some(inline) = &entry.inline else {
            continue;
        };
        let cost = spent - printed[at].cost() + inline.cost();
        if cost <= room && body_spent + entry.body_cost <= room / 2 {
            spent = cost;
            body_spent += entry.body_cost;
            printed[at] = inline.clone();
        }
    }
    CutOutline {
        lines: printed,
        sections,
        stub_cost,
    }
}

fn entries(lines: &[ViewLine], from: usize, boundary: Option<usize>) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (position, line) in lines.iter().enumerate().skip(from) {
        let Some(level) = line.heading else {
            continue;
        };
        let held = lines[position + 1..]
            .iter()
            .take_while(|next| next.heading.is_none_or(|next| next > level))
            .count();
        let heading = line.heading_text();
        let mut text: String = heading.chars().take(HEADING_CHARS).collect();
        if text.chars().count() < heading.chars().count() {
            text.push('…');
        }
        let text = escape_text(&text);
        let hashes = "#".repeat(usize::from(level.clamp(1, 6)));
        let mut printed = ViewLine::new(
            0,
            format!("{hashes} {text} ({held} lines, --from {})", line.number),
        );
        printed.number = line.number;
        printed.heading = Some(level);
        printed.plain = Some(heading);
        let body = &lines[position + 1..position + 1 + held];
        let body_cost = body.iter().map(ViewLine::cost).sum();
        let body_lines: usize = body.iter().map(|line| line.text.split('\n').count()).sum();
        let inline = if (1..=2).contains(&body_lines)
            && body_cost <= BODY_CHARS
            && !has_token(&line.text)
            && body.last().is_none_or(|line| Some(line.number) != boundary)
            && body
                .iter()
                .all(|line| line.heading.is_none() && line.cut.is_none() && !has_token(&line.text))
        {
            let mut inline = printed.clone();
            inline.text = format!("{hashes} {text}");
            for line in body {
                inline.text.push('\n');
                inline.text.push_str(&line.printed());
            }
            Some(inline)
        } else {
            None
        };
        entries.push(Entry {
            level,
            number: line.number,
            line: printed,
            inline,
            body_cost,
        });
    }
    entries
}

fn has_token(text: &str) -> bool {
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '[' {
            return true;
        }
    }
    false
}

/// The indexes of the entries to list, in page order: those of every level
/// up to the deepest one whose headings all fit, then as many of the next
/// level's as the room still holds, in order. Also that next level, so the
/// last line can point at its first heading left out rather than at a
/// deeper one.
fn choose(entries: &[Entry], room: usize) -> (Vec<usize>, Option<u8>) {
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
    (chosen, next)
}
