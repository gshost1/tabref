use tabref_core::AppError;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::snapshot_view::snapshot_view;
use crate::view::view_request::ViewRequest;
use crate::view::view_scope::ViewScope;

/// Takes a new snapshot of the current tab and prints its page view. With
/// `--root`, prints only that element's part of the page, with refs of the
/// new snapshot; `--rows`, `--from` and `--full` pick the rows, the first
/// line and whole lines.
pub(crate) fn run(session: &mut Session, request: &ViewRequest) -> Result<Outcome, AppError> {
    let scope = match &request.root {
        Some(root) => {
            let target = session.resolve_ref(root)?;
            ViewScope::Element {
                target_id: target.token.target_id.clone(),
                backend_id: target.token.backend_node_id,
            }
        }
        None => ViewScope::Page,
    };
    let captured = session.capture()?;
    let options = request.snap_options(&captured.info.url)?;
    snapshot_view(session, &captured, &options, &scope)
}
