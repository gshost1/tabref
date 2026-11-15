use std::collections::HashMap;

use super::cut_report::CutReport;
use super::view_line::ViewLine;

/// Lines of one navigation, header, footer or sidebar a view keeps when
/// the page has a main landmark.
const CHROME_LINES: usize = 6;
const MAX_CUT_HEADINGS: usize = 80;

/// Fits `lines` into `budget` characters. Each chrome landmark keeps its
/// first lines and a note of how many it drops, unless `whole_chrome`.
/// Then lines print in order until the budget is spent; after that only
/// headings print, each with the number of lines its section held, so the
/// rest of the page stays countable and reachable by name.
pub(crate) fn fit(
    lines: Vec<ViewLine>,
    budget: usize,
    whole_chrome: bool,
    has_main: bool,
) -> (Vec<ViewLine>, CutReport) {
    let mut report = CutReport {
        lines_total: lines.len(),
        chars_total: lines.iter().map(ViewLine::cost).sum(),
        ..CutReport::default()
    };
    let lines = if whole_chrome || !has_main {
        lines
    } else {
        cap_chrome(lines, &mut report)
    };
    let mut kept = Vec::with_capacity(lines.len());
    let mut spent = 0usize;
    let mut cutting = false;
    let mut shown = 0usize;
    for (position, line) in lines.iter().enumerate() {
        if !cutting && spent + line.cost() > budget {
            cutting = true;
        }
        if !cutting {
            spent += line.cost();
            shown += 1;
            kept.push(line.clone());
            continue;
        }
        if line.heading.is_none() {
            continue;
        }
        report.sections_cut += 1;
        if report.first_cut_heading.is_none() {
            report.first_cut_heading = Some(heading_text(&line.text));
        }
        if report.sections_cut > MAX_CUT_HEADINGS {
            continue;
        }
        let held = lines[position + 1..]
            .iter()
            .take_while(|next| next.heading.is_none_or(|level| Some(level) > line.heading))
            .count();
        let mut marked = line.clone();
        marked.text.push_str(&format!(" [not shown: {held} lines]"));
        kept.push(marked);
    }
    if report.sections_cut > MAX_CUT_HEADINGS {
        let more = report.sections_cut - MAX_CUT_HEADINGS;
        kept.push(ViewLine {
            depth: 0,
            text: format!("… {more} more sections not shown"),
            heading: None,
            chrome: None,
        });
    }
    report.lines_shown = if cutting {
        shown
    } else {
        report.lines_total - report.chrome_cut
    };
    report.chars_shown = kept.iter().map(ViewLine::cost).sum();
    (kept, report)
}

fn cap_chrome(lines: Vec<ViewLine>, report: &mut CutReport) -> Vec<ViewLine> {
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
            capped.push(ViewLine {
                depth: line.depth,
                text: format!(
                    "… {} more lines here: tabref snap --all",
                    total - CHROME_LINES
                ),
                heading: None,
                chrome: Some(group),
            });
        }
    }
    capped
}

fn heading_text(text: &str) -> String {
    text.trim_start_matches('#').trim().to_owned()
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
