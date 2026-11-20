use tabref_page::{PageOutline, SkippedFrame};

use super::cut_report::CutReport;
use super::cut_stats::CutStats;
use super::view_line::ViewLine;
use super::view_options::ViewOptions;

/// The view's last line lists its size and what was left out. A budget
/// cut or `--to` points to `--from` at the first omitted line. Other
/// hints cover omitted rows and navigation, shortened lines, and
/// frames the snapshot could not read in time.
pub(crate) fn footer_line(
    report: &CutReport,
    stats: &CutStats,
    kept: &[ViewLine],
    options: &ViewOptions,
    frames_unread: usize,
) -> String {
    let size = format!("{} lines, {}", report.lines_total, kilo(report.chars_total));
    let shortened: Vec<&ViewLine> = kept.iter().filter(|line| line.cut.is_some()).collect();
    if report.is_complete() && stats.rows_cut == 0 && shortened.is_empty() && frames_unread == 0 {
        return format!("-- {size} chars, all shown");
    }
    let route = &options.route;
    let full = options.asks_full();
    let mut cut = Vec::new();
    let mut more = Vec::new();
    if let Some(from) = report.budget_from {
        cut.push(format!("lines {from}- over the budget"));
    }
    if let Some(next) = report.after_to {
        cut.push(format!("lines {next}- after --to"));
    }
    if let Some(next) = report.budget_from.or(report.after_to) {
        more.push(route.starting_at(next, full));
    }
    if report.sections.count > 0 {
        if report.sections.listed < report.sections.count {
            cut.push(format!(
                "{} sections after it, {} listed",
                report.sections.count, report.sections.listed
            ));
        } else {
            cut.push(format!("{} sections as headings", report.sections.count));
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
    if frames_unread > 0 {
        cut.push(format!("{frames_unread} frames not read in time"));
        more.push(route.command.clone());
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

/// Frames of the page the snapshot left out because they did not answer in
/// time or could not be read, in or out of process: their controls carry
/// no refs, so a view with any of them is not complete.
pub(crate) fn unread_frames(skipped: &[SkippedFrame]) -> usize {
    skipped
        .iter()
        .filter(|frame| frame.reason.is_unread())
        .count()
}

/// Whether the view shows the whole page: nothing cut by the budget or the
/// chrome limit, and no frame left unread.
pub(crate) fn is_view_complete(report: &CutReport, skipped: &[SkippedFrame]) -> bool {
    report.is_complete() && unread_frames(skipped) == 0
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
