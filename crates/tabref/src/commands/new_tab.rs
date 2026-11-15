//! `open --new-tab`: the address resolved before any tab opens, the new
//! tab made current, and an error after it opened that says so.

use serde_json::json;
use tabref_cdp::{PageSession, activate_target, create_background_tab};
use tabref_core::{AppError, Deadline};

use super::open::{is_relative, normalize_url};
use super::page_url::resolve_url;
use crate::session::Session;

/// The full address a new tab loads: `address` as `open` reads it, a
/// relative one resolved against the current tab's address as the tab
/// listing gives it, so the current tab is never claimed. A relative
/// address with no page to resolve against (a blank tab, no tab) is
/// refused before a tab opens.
pub(crate) fn new_tab_address(session: &mut Session, address: &str) -> Result<String, AppError> {
    let url = normalize_url(address);
    if !is_relative(&url) {
        return Ok(url);
    }
    let resolved = match session.current_url_unclaimed()? {
        Some(base) => resolve_url(&base, &url),
        None => url,
    };
    if is_relative(&resolved) {
        return Err(AppError::invalid_args(
            "a relative address needs a current page; give a full URL",
        ));
    }
    Ok(resolved)
}

/// Opens a blank tab, brings it to the front and makes it the current tab,
/// for an address to load in. Returns its index, as `tabref tabs` lists
/// it, and its page session.
pub(crate) fn open_tab(
    session: &mut Session,
    deadline: Deadline,
) -> Result<(usize, PageSession), AppError> {
    let tab = create_background_tab(session.connection(), "about:blank", deadline)?;
    activate_target(session.connection(), &tab, deadline)?;
    session.use_tab(&tab)?;
    let index = session
        .tabs()?
        .iter()
        .position(|listed| listed.target_id == tab)
        .ok_or_else(|| {
            AppError::internal(format!("the new tab {tab} is not in the tab listing"))
        })?;
    Ok((index, session.page()?))
}

/// An error after the new tab opened: it names the tab, which stays open
/// and current, and asks for the same address without `--new-tab`, as
/// repeating the command would open another tab. Never marked retryable.
pub(crate) fn in_new_tab(error: AppError, index: usize, url: &str) -> AppError {
    let mut details = error.details.clone().unwrap_or_else(|| json!({}));
    if let Some(object) = details.as_object_mut() {
        object.remove("retryable");
        object.insert("new_tab_index".into(), json!(index));
    }
    error
        .with_suggestion(format!(
            "new tab [{index}] is now current; run `open {url}` without --new-tab"
        ))
        .with_details(details)
}

#[cfg(test)]
#[path = "new_tab_tests.rs"]
mod tests;
