use serde_json::json;
use tabref_cdp::PageTarget;
use tabref_core::AppError;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// Lists the open tabs by index; `*` marks the current one. An app that
/// embeds Chrome and shows no window page has no tabs, and none is current.
pub(crate) fn run(session: &mut Session) -> Result<Outcome, AppError> {
    let current = match session.tab_id() {
        Ok(tab) => tab,
        Err(error) if error.kind() == Some("cdp_app_has_no_page") => String::new(),
        Err(error) => return Err(error),
    };
    let tabs = session.tabs()?;
    Ok(tab_list(&tabs, &current))
}

pub(crate) fn tab_list(tabs: &[PageTarget], current: &str) -> Outcome {
    let lines: Vec<String> = tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let mark = if tab.target_id == current { '*' } else { ' ' };
            format!(
                "{mark}[{index}] {} {} id={}",
                quote_text(&tab.title),
                tab.url,
                short_id(&tab.target_id)
            )
        })
        .collect();
    let data: Vec<_> = tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            json!({
                "index": index,
                "id": tab.target_id,
                "url": tab.url,
                "title": tab.title,
                "current": tab.target_id == current,
            })
        })
        .collect();
    Outcome::new(lines.join("\n"), json!({ "tabs": data }))
}

/// The first eight characters of a target id, enough for `tab <id>`.
fn short_id(target_id: &str) -> &str {
    target_id.get(..8).unwrap_or(target_id)
}
