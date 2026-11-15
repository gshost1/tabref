use tabref_page::{PageOutline, SkippedFrame};

use super::cut_report::CutReport;
use super::cut_stats::CutStats;
use super::view_line::ViewLine;
use super::view_options::ViewOptions;
use super::view_route::quoted_argument;

/// The view's last line: its size, and when anything was left out, every
/// kind of cut with the command that shows it: lines past the budget,
/// sections left as headings, rows past the window, chrome, and lines cut
/// short (each of those also carries its own marker).
pub(crate) fn footer_line(
    report: &CutReport,
    stats: &CutStats,
    kept: &[ViewLine],
    options: &ViewOptions,
) -> String {
    let size = format!("{} lines, {}", report.lines_total, kilo(report.chars_total));
    let shortened: Vec<&ViewLine> = kept.iter().filter(|line| line.cut.is_some()).collect();
    if report.is_complete() && stats.rows_cut == 0 && shortened.is_empty() {
        return format!("-- {size} chars, all shown");
    }
    let route = &options.route;
    let full = options.asks_full();
    let mut cut = Vec::new();
    let mut more = Vec::new();
    if let Some(from) = report.budget_from {
        cut.push(format!("lines {from}- over the budget"));
        more.push(route.starting_at(from, full));
    }
    if report.sections.count > 0 {
        cut.push(format!("{} sections as headings", report.sections.count));
        if let Some(heading) = &report.sections.first_heading {
            more.push(format!(
                "tabref read --section {}",
                quoted_argument(heading)
            ));
        }
    }
    if stats.rows_cut > 0 {
        cut.push(format!("{} rows", stats.rows_cut));
        if let Some(next) = stats.next_row {
            more.push(route.rows_from(next, full));
        }
    }
    if report.chrome_cut > 0 {
        cut.push(format!("{} navigation lines", report.chrome_cut));
        more.push(format!("{} --all", route.command));
    }
    if let Some(first) = shortened.first() {
        cut.push(format!("{} lines cut short", shortened.len()));
        more.push(route.starting_at(first.number, true));
    }
    more.push("tabref find <text>".to_owned());
    let shown = if report.lines_shown < report.lines_total {
        format!(
            "shown {} of {} lines ({} of {} chars)",
            report.lines_shown,
            report.lines_total,
            kilo(report.chars_shown),
            kilo(report.chars_total)
        )
    } else {
        format!("{size} chars")
    };
    format!(
        "-- {shown}; cut: {}; more: {}",
        cut.join(", "),
        more.join(" | ")
    )
}

/// One line for the frames the view left out, or `None` when it left none.
/// The outline counts the frames of the page's DOM; an out-of-process frame
/// the snapshot could not read in time has no DOM to show and is counted
/// there as cross-origin, so the snapshot's list moves it to "not read in
/// time". A frame the snapshot left out in-process is still in the DOM, and
/// the outline has already judged it.
pub(crate) fn frames_line(outline: &PageOutline, skipped: &[SkippedFrame]) -> Option<String> {
    let mut frames = outline.frames;
    let timed_out = skipped
        .iter()
        .filter(|frame| frame.out_of_process && frame.reason.is_unread())
        .count();
    let moved = timed_out.min(frames.cross_origin);
    frames.cross_origin -= moved;
    frames.unread += moved;
    let mut parts = Vec::new();
    if frames.cross_origin > 0 {
        parts.push(format!(
            "{} cross-origin (ads, widgets)",
            frames.cross_origin
        ));
    }
    if frames.tiny > 0 {
        parts.push(format!("{} tiny or hidden", frames.tiny));
    }
    if frames.empty > 0 {
        parts.push(format!("{} empty", frames.empty));
    }
    if frames.unread > 0 {
        parts.push(format!("{} not read in time", frames.unread));
    }
    if parts.is_empty() {
        return None;
    }
    let total = frames.skipped();
    Some(format!("frames: {total} not shown: {}", parts.join(", ")))
}

fn kilo(chars: usize) -> String {
    if chars < 1_000 {
        return chars.to_string();
    }
    format!("{:.1}k", chars as f64 / 1_000.0)
}

#[cfg(test)]
#[path = "view_footer_tests.rs"]
mod tests;
