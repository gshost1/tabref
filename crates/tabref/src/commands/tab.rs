use tabref_cdp::activate_target;
use tabref_core::AppError;
use tabref_page::parse_capture;

use crate::output::outcome::Outcome;
use crate::output::page_report::page_report;
use crate::session::Session;
use crate::session::tab_choice::find_tab;
use crate::view::bot_check::bot_check_view;
use crate::view::view_mode::ViewMode;

/// Switches to a tab by index or id, brings it to the front, and prints a
/// snapshot of it.
pub(crate) fn run(session: &mut Session, hint: &str) -> Result<Outcome, AppError> {
    let tabs = session.tabs()?;
    let tab = find_tab(&tabs, hint)?;
    let deadline = session.deadline();
    activate_target(session.connection(), &tab, deadline)?;
    session.use_tab(&tab)?;
    let captured = session.capture()?;
    if let Some(dom) = captured.dom_captures.get(&tab)
        && let Some(report) = bot_check_view(&captured.info, parse_capture(dom, &tab).first())
    {
        return Ok(report);
    }
    Ok(page_report(&captured, ViewMode::Interactive))
}
