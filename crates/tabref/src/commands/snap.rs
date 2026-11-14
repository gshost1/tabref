use tabref_core::AppError;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::row_window::RowWindow;
use crate::view::snapshot_view::snapshot_view;
use crate::view::view_options::ViewOptions;
use crate::view::view_scope::ViewScope;

/// Takes a new snapshot of the current tab and prints its page view. With
/// `root`, prints only that element's part of the page, with refs of the
/// new snapshot. `rows` picks the rows of long tables and lists.
pub(crate) fn run(
    session: &mut Session,
    all: bool,
    root: Option<&str>,
    rows: Option<&str>,
) -> Result<Outcome, AppError> {
    let rows = rows.map(RowWindow::parse).transpose()?.unwrap_or_default();
    let scope = match root {
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
    let options = ViewOptions::snap(&captured.info.url, all, rows);
    snapshot_view(session, &captured, &options, &scope)
}
