use std::collections::HashMap;

use super::cut_marker::cut_marker;
use super::cut_outline::cut_outline;
use super::cut_report::CutReport;
use super::view_line::ViewLine;
use super::view_options::ViewOptions;

/// Lines of one navigation, header, footer or sidebar a view keeps when
/// the page has a main landmark.
const CHROME_LINES: usize = 6;
/// Characters a cut view keeps for its header, frames and footer lines, at
/// most a tenth of the budget.
const FRAME_ROOM: usize = 400;
/// Characters the cut marker line is allowed before it is written, at most
/// a tenth of the budget.
const MARKER_ROOM: usize = 100;
/// The most of a cut view's budget the outline of its later headings may
/// take from the content: one part in this many.
const OUTLINE_SHARE: usize = 5;
/// The most of a cut view's budget its kept lines (the rows a short
/// `--rows` window asked for) may take after the cut: one part in this
/// many.
const KEEP_SHARE: usize = 2;

/// Fits `lines` into the budget of `options`. Each chrome landmark keeps
/// its first lines and a note of how many it drops, unless chrome prints
/// whole. A view that fits prints whole. Otherwise lines print in order
/// until the content's part of the budget is spent; a marker line says
/// where the cut starts and which command prints on from there, and a
/// compact outline of the headings after it (see [`cut_outline`]) keeps the
/// rest of the page countable and reachable. Kept lines the cut would drop
/// print right after the marker, their room taken from the content.
/// Content, marker, kept lines, outline and room for the header and footer
/// together stay within the budget.
pub(crate) fn fit(
    lines: Vec<ViewLine>,
    options: &ViewOptions,
    has_main: bool,
) -> (Vec<ViewLine>, CutReport) {
    let mut report = CutReport {
        lines_total: lines.len(),
        chars_total: lines.iter().map(ViewLine::cost).sum(),
        ..CutReport::default()
    };
    let lines = if options.whole_chrome || !has_main {
        lines
    } else {
        cap_chrome(lines, options, &mut report)
    };
    let route = &options.route;
    let full = options.asks_full();
    let total: usize = lines.iter().map(ViewLine::cost).sum();
    if total <= options.budget {
        report.lines_shown = report.lines_total - report.chrome_cut;
        report.chars_shown = total;
        return (lines, report);
    }
    let tenth = options.budget / 10;
    let room = options.budget.saturating_sub(FRAME_ROOM.min(tenth));
    let share = room / OUTLINE_SHARE;
    let wanted = cut_outline(&lines, cut_index(&lines, room), share, route, full).stub_cost;
    let content_room = room.saturating_sub(wanted.min(share) + MARKER_ROOM.min(tenth));
    let (mut cut, mut held) = cut_keeping(&lines, content_room, room / KEEP_SHARE);
    let mut reserve: usize = held.iter().map(ViewLine::cost).sum();
    let mut kept: Vec<ViewLine> = lines[..cut].to_vec();
    let mut spent: usize = kept.iter().map(ViewLine::cost).sum();
    let last = lines.last().map_or(0, |line| line.number);
    let mut held_count = 0;
    while let Some(first) = lines.get(cut) {
        let marker = cut_marker(&lines[cut..], last, route, full);
        if spent + marker.cost() + reserve > room {
            if let Some(line) = kept.pop() {
                spent -= line.cost();
                cut -= 1;
                if line.keep {
                    reserve += line.cost();
                    held.insert(0, line);
                }
                continue;
            }
        } else {
            spent += marker.cost();
            kept.push(marker);
        }
        if spent + reserve <= room {
            spent += reserve;
            held_count = held.len();
            kept.append(&mut held);
        }
        report.budget_from = Some(first.number);
        let outline = cut_outline(&lines, cut, room - spent, route, full);
        report.sections = outline.sections;
        kept.extend(outline.lines);
        break;
    }
    report.lines_shown = cut + held_count;
    report.chars_shown = kept.iter().map(ViewLine::cost).sum();
    (kept, report)
}

/// The index of the first line that does not fit in `room` characters,
/// taking lines in order.
fn cut_index(lines: &[ViewLine], room: usize) -> usize {
    let mut spent = 0usize;
    for (position, line) in lines.iter().enumerate() {
        spent += line.cost();
        if spent > room {
            return position;
        }
    }
    lines.len()
}

/// Where the cut falls when the kept lines after it take their room from
/// the content's `room`, and those kept lines, as many in order as fit in
/// `most` characters.
fn cut_keeping(lines: &[ViewLine], room: usize, most: usize) -> (usize, Vec<ViewLine>) {
    let mut reserve = 0usize;
    loop {
        let cut = cut_index(lines, room.saturating_sub(reserve));
        let mut spent = 0usize;
        let held: Vec<ViewLine> = lines[cut..]
            .iter()
            .filter(|line| line.keep)
            .take_while(|line| {
                spent += line.cost();
                spent <= most
            })
            .cloned()
            .collect();
        let cost: usize = held.iter().map(ViewLine::cost).sum();
        if cost <= reserve {
            return (cut, held);
        }
        reserve = cost;
    }
}

fn cap_chrome(
    lines: Vec<ViewLine>,
    options: &ViewOptions,
    report: &mut CutReport,
) -> Vec<ViewLine> {
    let mut totals: HashMap<usize, usize> = HashMap::new();
    for line in &lines {
        if let Some(group) = line.chrome {
            *totals.entry(group).or_default() += 1;
        }
    }
    let mut seen: HashMap<usize, usize> = HashMap::new();
    let mut capped = Vec::with_capacity(lines.len());
    for line in lines {
        let Some(group) = line.chrome else {
            capped.push(line);
            continue;
        };
        let place = seen.entry(group).or_default();
        *place += 1;
        let total = totals.get(&group).copied().unwrap_or(0);
        if *place <= CHROME_LINES {
            capped.push(line);
            continue;
        }
        report.chrome_cut += 1;
        if *place == CHROME_LINES + 1 {
            let mut note = ViewLine::new(
                line.depth,
                format!(
                    "… {} more lines here: {} --all",
                    total - CHROME_LINES,
                    options.route.command
                ),
            );
            note.chrome = Some(group);
            note.number = line.number;
            capped.push(note);
        }
    }
    capped
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "budget_keep_tests.rs"]
mod keep_tests;
