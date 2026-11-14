use tabref_page::PageOutline;

use super::cut_report::CutReport;
use super::cut_stats::CutStats;

/// The view's last line: its size, and when anything was left out, what
/// and the commands that show it.
pub(crate) fn footer_line(report: &CutReport, stats: &CutStats) -> String {
    let size = format!("{} lines, {}", report.lines_total, kilo(report.chars_total));
    if report.is_complete() && stats.rows_cut == 0 {
        return format!("-- {size} chars, all shown");
    }
    let mut cut = Vec::new();
    let mut more = Vec::new();
    if report.sections_cut > 0 {
        cut.push(format!("{} sections after the budget", report.sections_cut));
        if let Some(heading) = &report.first_cut_heading {
            more.push(format!("tabref read --section \"{heading}\""));
        }
    }
    if stats.rows_cut > 0 {
        cut.push(format!("{} rows", stats.rows_cut));
        if let Some(next) = stats.next_row {
            more.push(format!("tabref snap --rows {next}-"));
        }
    }
    if report.chrome_cut > 0 {
        cut.push(format!("{} navigation lines", report.chrome_cut));
        more.push("tabref snap --all".to_owned());
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
    format!("-- {shown}; cut: {}; more: {}", cut.join(", "), more.join(" | "))
}

/// One line for the frames the view left out, or `None` when it left none.
pub(crate) fn frames_line(outline: &PageOutline, unreadable: usize) -> Option<String> {
    let frames = outline.frames;
    let mut parts = Vec::new();
    if frames.cross_origin > 0 {
        parts.push(format!("{} cross-origin (ads, widgets)", frames.cross_origin));
    }
    if frames.tiny > 0 {
        parts.push(format!("{} tiny or hidden", frames.tiny));
    }
    if frames.empty > 0 {
        parts.push(format!("{} empty", frames.empty));
    }
    let unread = frames.unread + unreadable;
    if unread > 0 {
        parts.push(format!("{unread} not read in time"));
    }
    if parts.is_empty() {
        return None;
    }
    let total = frames.skipped() + unreadable;
    Some(format!("frames: {total} not shown: {}", parts.join(", ")))
}

fn kilo(chars: usize) -> String {
    if chars < 1_000 {
        return chars.to_string();
    }
    format!("{:.1}k", chars as f64 / 1_000.0)
}
