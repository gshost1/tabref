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
    Ok(report(&tab, &remaining, &now, list))
}

/// What `close` prints: the closed tab's id, the count of tabs left and the
/// current one by index and id, with no title or URL; `list` adds the tabs
/// left as `tabs` lists them. The JSON has `closed`, `tabs_left` and
/// `current` either way, and `tabs` only with `list`.
fn report(closed: &str, remaining: &[PageTarget], now: &str, list: bool) -> Outcome {
    let current = remaining
        .iter()
        .position(|open| open.target_id == now)
        .map(|index| (index, &remaining[index].target_id));
    let left = remaining.len();
    let plural = if left == 1 { "" } else { "s" };
    let head = format!("closed id={}", short_id(closed));
    let mut text = match current {
        _ if left == 0 => format!("{head}, the last tab"),
        Some((index, id)) => format!(
            "{head}; {left} tab{plural} left, current [{index}] id={}",
            short_id(id)
        ),
        None => format!("{head}; {left} tab{plural} left"),
    };
    let mut data = json!({
        "closed": closed,
        "tabs_left": left,
        "current": current.map(|(index, id)| json!({ "index": index, "id": id })),
    });
    if list && left > 0 {
        let listed = tab_list(remaining, now);
        text = format!("{text}:\n{}", listed.text);
        data["tabs"] = listed.data["tabs"].clone();
    } else if list {
        data["tabs"] = json!([]);
    } else if left > 0 {
        text.push_str("; 'tabref tabs' lists them");
    }
    Outcome::new(text, data)
}

#[cfg(test)]
#[path = "close_tests.rs"]
mod tests;
