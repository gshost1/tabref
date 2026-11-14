use tabref_cdp::close_target;
use tabref_core::AppError;

use super::tabs::tab_list;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::tab_choice::find_tab;

/// Closes a tab, the current one by default, and lists the tabs left.
pub(crate) fn run(session: &mut Session, hint: Option<&str>) -> Result<Outcome, AppError> {
    let current = session.tab_id()?;
    let tab = match hint {
        Some(hint) => find_tab(&session.tabs()?, hint)?,
        None => current.clone(),
    };
    let deadline = session.deadline();
    close_target(session.connection(), &tab, deadline)?;
    let remaining: Vec<_> = session
        .tabs()?
        .into_iter()
        .filter(|open| open.target_id != tab)
        .collect();
    let now = if tab == current {
        session.forget_tab();
        match remaining.first() {
            Some(first) => {
                session.use_tab(&first.target_id)?;
                first.target_id.clone()
            }
            None => String::new(),
        }
    } else {
        current
    };
    let mut outcome = tab_list(&remaining, &now);
    let note = if remaining.is_empty() {
        "closed the last tab".to_owned()
    } else {
        "closed; tabs left:".to_owned()
    };
    outcome.text = format!("{note}\n{}", outcome.text).trim_end().to_owned();
    Ok(outcome)
}
