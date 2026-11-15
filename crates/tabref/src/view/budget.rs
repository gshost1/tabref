use std::collections::HashMap;

use super::cut_report::CutReport;
use super::view_line::ViewLine;
use super::view_options::ViewOptions;

/// Lines of one navigation, header, footer or sidebar a view keeps when
/// the page has a main landmark.
const CHROME_LINES: usize = 6;
const MAX_CUT_HEADINGS: usize = 80;

/// Fits `lines` into the budget of `options`. Each chrome landmark keeps
/// its first lines and a note of how many it drops, unless chrome prints
/// whole. Then lines print in order until the budget is spent; a marker
/// line says where the cut starts and which command prints on from there.
/// After it only headings print, each with the number of lines its section
/// held and the line it starts at, so the rest of the page stays countable
/// and reachable.
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
    let mut kept = Vec::with_capacity(lines.len());
    let mut spent = 0usize;
    let mut shown = 0usize;
    for (position, line) in lines.iter().enumerate() {
        if report.budget_from.is_none() && spent + line.cost() > options.budget {
            report.budget_from = Some(line.number);
            let last = lines.last().map_or(line.number, |last| last.number);
            kept.push(ViewLine::new(
                0,
                format!(
                    "… lines {}-{last} cut here to fit: {}",
                    line.number,
                    route.starting_at(line.number, full)
                ),
            ));
        }
        if report.budget_from.is_none() {
            spent += line.cost();
            shown += 1;
            kept.push(line.clone());
            continue;
        }
        if line.heading.is_none() {
            continue;
        }
        report.sections.count += 1;
        if report.sections.first_heading.is_none() {
            report.sections.first_heading = Some(line.heading_text());
        }
        if report.sections.count > MAX_CUT_HEADINGS {
            if report.sections.count == MAX_CUT_HEADINGS + 1 {
                kept.push(ViewLine::new(
                    0,
                    format!(
                        "… more sections from line {}: {}",
                        line.number,
                        route.starting_at(line.number, full)
                    ),
                ));
            }
            continue;
        }
        let held = lines[position + 1..]
            .iter()
            .take_while(|next| next.heading.is_none_or(|level| Some(level) > line.heading))
            .count();
        let mut marked = line.clone();
        marked.cut = None;
        marked.text.push_str(&format!(
            " [not shown: {held} lines, --from {}]",
            line.number
        ));
        kept.push(marked);
    }
    report.lines_shown = if report.budget_from.is_some() {
        shown
    } else {
        report.lines_total - report.chrome_cut
    };
    report.chars_shown = kept.iter().map(ViewLine::cost).sum();
    (kept, report)
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
