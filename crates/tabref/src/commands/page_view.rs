use tabref_core::AppError;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::snapshot_view::snapshot_view;
use crate::view::view_request::ViewRequest;
use crate::view::view_scope::ViewScope;

/// A new snapshot of the current tab as `open` and `snap` print it, for an
/// action that led to another page or a batch step that failed.
pub(crate) fn current_page_view(session: &mut Session) -> Result<Outcome, AppError> {
    let captured = session.capture()?;
    let options = ViewRequest::default().snap_options(&captured.info.url)?;
    snapshot_view(session, &captured, &options, &ViewScope::Page)
}
