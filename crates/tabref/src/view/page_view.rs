use serde_json::json;
use tabref_core::AppError;
use tabref_page::{Block, OutlineInput, PageOutline, build_outline, build_outline_at};

use super::block_renderer::BlockRenderer;
use super::block_titles::title_of;
use super::budget::fit;
use super::main_content::{has_main, main_block, text_size};
use super::node_line::quote_text;
use super::ref_style::RefStyle;
use super::section::section_lines;
use super::view_footer::{footer_line, frames_line, is_view_complete, unread_frames};
use super::view_line::ViewLine;
use super::view_options::ViewOptions;
use super::view_scope::ViewScope;
use super::view_subject::ViewSubject;
use crate::output::outcome::Outcome;

/// Prints the page from `input` as `options` and `scope` say: a header,
/// dialogs and overlays first, the content in reading order within the
/// budget, the frames left out, and a footer that says what was cut and
/// the command that shows it.
pub(crate) fn page_view(
    subject: &ViewSubject<'_>,
    input: &OutlineInput,
    options: &ViewOptions,
    scope: &ViewScope,
) -> Result<Outcome, AppError> {
    let outline = match scope {
        ViewScope::Element {
            target_id,
            backend_id,
        } => build_outline_at(input, target_id, *backend_id).ok_or_else(|| {
            AppError::stale_ref_because("that element is no longer in the page".to_owned())
        })?,
        _ => build_outline(input),
    };
    let body: &Block = match scope {
        ViewScope::Main => main_block(&outline.body),
        _ => &outline.body,
    };
    let mut renderer = BlockRenderer::new(options);
    let body_size = text_size(body);
    for dialog in &outline.dialogs {
        if text_size(dialog) > body_size {
            renderer.render_block(dialog, 0);
        } else if *scope == ViewScope::Page {
            renderer.render_dialog(dialog);
        } else {
            let title = title_of(dialog);
            renderer.push(
                0,
                format!("{title} covers the page: tabref snap shows its buttons"),
                None,
            );
        }
    }
    renderer.render_children(body, 0);
    let stats = renderer.stats;
    let mut lines = renderer.lines;
    if let ViewScope::Section(wanted) = scope {
        lines = section_lines(lines, wanted).ok_or_else(|| no_section(wanted))?;
    }
    for (position, line) in lines.iter_mut().enumerate() {
        line.number = position + 1;
        if let Some(cut) = &line.cut {
            let command = options.route.starting_at(line.number, true);
            line.text.push_str(&format!(" …({cut}: {command})"));
        }
    }
    let lines: Vec<ViewLine> = lines.into_iter().skip(options.route.from - 1).collect();
    let main = *scope == ViewScope::Page && has_main(&outline.body);
    let (kept, report) = fit(lines, options, main);
    let mut text = header(subject, options);
    for line in &kept {
        text.push('\n');
        text.push_str(&line.printed());
    }
    if let Some(frames) = frames_line(&outline, subject.skipped_frames) {
        text.push('\n');
        text.push_str(&frames);
    }
    text.push('\n');
    text.push_str(&footer_line(
        &report,
        &stats,
        &kept,
        options,
        unread_frames(subject.skipped_frames),
    ));
    Ok(Outcome::new(text, data(subject, &kept, &report, &outline)))
}

fn header(subject: &ViewSubject<'_>, options: &ViewOptions) -> String {
    let mut text = format!(
        "page {} {}",
        quote_text(subject.info.title.trim()),
        subject.info.url
    );
    if let (RefStyle::Short, Some(snapshot)) = (options.refs, subject.snapshot_id) {
        if subject.ref_count > 0 {
            text.push_str(&format!(" · refs eN = @{snapshot}:eN"));
        }
    }
    text
}

fn data(
    subject: &ViewSubject<'_>,
    kept: &[ViewLine],
    report: &super::cut_report::CutReport,
    outline: &PageOutline,
) -> serde_json::Value {
    json!({
        "snapshot_id": subject.snapshot_id,
        "url": subject.info.url,
        "title": subject.info.title,
        "ref_count": subject.ref_count,
        "unreadable_frames": unread_frames(subject.skipped_frames),
        "skipped_frames": subject.skipped_frames,
        "lines": kept.iter().map(ViewLine::printed).collect::<Vec<_>>(),
        "complete": is_view_complete(report, subject.skipped_frames),
        "lines_total": report.lines_total,
        "lines_shown": report.lines_shown,
        "chars_total": report.chars_total,
        "frames_skipped": outline.frames.skipped(),
        "first_cut_line": report.budget_from,
    })
}

fn no_section(wanted: &str) -> AppError {
    AppError::invalid_args(format!("no heading on the page contains {wanted:?}"))
        .with_suggestion("Copy a heading from the view, or run tabref find <text>")
}

#[cfg(test)]
#[path = "page_view_tests.rs"]
mod tests;
