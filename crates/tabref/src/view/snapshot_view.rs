use tabref_core::AppError;
use tabref_page::{PageInfo, capture_dom, page_text};

use super::bot_check::bot_check_view;
use super::outline_source::outline_input;
use super::page_view::page_view;
use super::view_mode::ViewMode;
use super::view_options::ViewOptions;
use super::view_scope::ViewScope;
use super::view_subject::ViewSubject;
use crate::output::outcome::Outcome;
use crate::output::page_report::page_report;
use crate::session::Session;
use crate::session::captured_snapshot::CapturedSnapshot;

/// The page view of a snapshot just taken: its refs, the page's DOM read
/// beside it, printed as `options` and `scope` say.
pub(crate) fn snapshot_view(
    session: &mut Session,
    captured: &CapturedSnapshot,
    options: &ViewOptions,
    scope: &ViewScope,
) -> Result<Outcome, AppError> {
    let input = match outline_input(session, Some(captured)) {
        Ok(input) => input,
        Err(error) => {
            let mut report = page_report(captured, ViewMode::Interactive);
            report.push_line(&format!(
                "note: the page view failed ({}), so this is the accessibility view",
                error.message
            ));
            return Ok(report);
        }
    };
    let subject = ViewSubject {
        info: &captured.info,
        snapshot_id: Some(&captured.snapshot_id),
        ref_count: captured.ref_count,
        skipped_frames: &captured.skipped_frames,
    };
    page_view(&subject, &input, options, scope)
}

/// The page view without a new snapshot, for reading: no refs, so nothing
/// of the snapshot store changes. The DOM is the daemon's last capture when
/// the page has provably not changed since.
pub(crate) fn reading_view(
    session: &mut Session,
    options: &ViewOptions,
    scope: &ViewScope,
) -> Result<Outcome, AppError> {
    let page = session.page()?;
    let deadline = session.deadline();
    let info = PageInfo::read(session.connection(), &page.target_id, deadline)?;
    let kept = session.reusable_capture();
    let input = match outline_input(session, kept.as_deref()) {
        Ok(mut input) => {
            input.refs.clear();
            input
        }
        Err(error) => return text_fallback(session, &info, &error),
    };
    let options = ViewOptions {
        page_url: info.url.clone(),
        ..options.clone()
    };
    let subject = ViewSubject {
        info: &info,
        snapshot_id: None,
        ref_count: 0,
        skipped_frames: &[],
    };
    page_view(&subject, &input, &options, scope)
}

const FALLBACK_TEXT: usize = 24_000;

/// Field reads use a direct element query. Check the top-level page for
/// a challenge without replacing the field result or allocating refs.
pub(crate) fn annotate_field_read(session: &mut Session, report: &mut Outcome) {
    let Ok(page) = session.page() else {
        return;
    };
    if let Some(captured) = session.reusable_capture()
        && let Some(document) = captured
            .main_dom(&page.target_id)
            .and_then(|trees| trees.first())
    {
        bot_check_view(&captured.info, Some(document), report);
        return;
    }
    let deadline = session.deadline();
    if let Ok(info) = PageInfo::read(session.connection(), &page.target_id, deadline)
        && let Ok(trees) = capture_dom(session.connection(), &page, deadline)
    {
        bot_check_view(&info, trees.first(), report);
    }
}

/// The page's rendered text, when the DOM capture behind the reading view
/// fails.
fn text_fallback(
    session: &mut Session,
    info: &PageInfo,
    error: &AppError,
) -> Result<Outcome, AppError> {
    let page = session.page()?;
    let deadline = session.deadline();
    let (text, cut) = page_text(session.connection(), &page, FALLBACK_TEXT, deadline)?;
    let mut out = format!(
        "page {} {}\n{text}",
        super::node_line::quote_text(&info.title),
        info.url
    );
    out.push_str(&format!(
        "\nnote: the reading view failed ({}), so this is the page's plain text",
        error.message
    ));
    if cut {
        out.push_str(&format!(
            "\n-- cut at {FALLBACK_TEXT} characters; more: tabref find <text>"
        ));
    }
    Ok(Outcome::new(
        out,
        serde_json::json!({ "url": info.url, "title": info.title, "text": text, "truncated": cut }),
    ))
}
