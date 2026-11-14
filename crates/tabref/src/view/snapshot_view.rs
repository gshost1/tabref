use tabref_core::AppError;
use tabref_page::PageInfo;

use super::outline_source::outline_input;
use super::page_view::page_view;
use super::view_options::ViewOptions;
use super::view_scope::ViewScope;
use super::view_subject::ViewSubject;
use crate::output::outcome::Outcome;
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
    let input = outline_input(session, Some(&captured.tree))?;
    let subject = ViewSubject {
        info: &captured.info,
        snapshot_id: Some(&captured.snapshot_id),
        ref_count: captured.ref_count,
        unreadable_frames: captured.unreadable_frames,
    };
    page_view(&subject, &input, options, scope)
}

/// The page view without a new snapshot, for reading: no refs, so nothing
/// of the snapshot store changes.
pub(crate) fn reading_view(
    session: &mut Session,
    options: &ViewOptions,
    scope: &ViewScope,
) -> Result<Outcome, AppError> {
    let page = session.page()?;
    let deadline = session.deadline();
    let info = PageInfo::read(session.connection(), &page.target_id, deadline)?;
    let input = outline_input(session, None)?;
    let options = ViewOptions {
        page_url: info.url.clone(),
        ..options.clone()
    };
    let subject = ViewSubject {
        info: &info,
        snapshot_id: None,
        ref_count: 0,
        unreadable_frames: 0,
    };
    page_view(&subject, &input, &options, scope)
}
