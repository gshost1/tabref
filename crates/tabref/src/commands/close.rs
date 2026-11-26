use serde_json::json;
use tabref_cdp::{PageTarget, close_target};
use tabref_core::AppError;

use super::app_guard::refuse_close_in_app;
use super::tabs::{short_id, tab_list};
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::tab_choice::find_tab;

/// Closes a tab, the current one by default, and says how many tabs are
/// left and which one is current now, by index and id. Their titles and
/// URLs are listed only with `list`: in the user's own browser they are the
/// user's tabs, which an agent should not read by the way. In an app that
/// embeds Chrome it is refused: its windows are the app.
pub(crate) fn run(
    session: &mut Session,
    hint: Option<&str>,
    list: bool,
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    refuse_close_in_app(session, deadline)?;
    let current = session.tab_id()?;
    let tab = match hint {
        Some(hint) => find_tab(&session.tabs()?, hint)?,
        None => current.clone(),
    };
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
    if list {
        let mut outcome = tab_list(&remaining, &now);
        let note = if remaining.is_empty() {
            "closed the last tab"
        } else {
            "closed; tabs left:"
        };
        outcome.text = format!("{note}\n{}", outcome.text).trim_end().to_owned();
        return Ok(outcome);
    }
    Ok(counted(&tab, &remaining, &now))
}

/// The default report: the count of tabs left and the current one, with
/// no title or URL.
fn counted(closed: &str, remaining: &[PageTarget], now: &str) -> Outcome {
    let current = remaining
        .iter()
        .position(|open| open.target_id == now)
        .map(|index| (index, &remaining[index].target_id));
    let text = match (remaining.len(), current) {
        (0, _) => "closed the last tab".to_owned(),
        (left, Some((index, id))) => format!(
            "closed; {left} tab{} left, current [{index}] id={}; 'tabref tabs' lists them",
            if left == 1 { "" } else { "s" },
            short_id(id)
        ),
        (left, None) => format!(
            "closed; {left} tab{} left; 'tabref tabs' lists them",
            if left == 1 { "" } else { "s" }
        ),
    };
    let data = json!({
        "closed": closed,
        "tabs_left": remaining.len(),
        "current": current.map(|(index, id)| json!({ "index": index, "id": id })),
    });
    Outcome::new(text, data)
}
