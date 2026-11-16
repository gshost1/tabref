use tabref_cdp::PageTarget;
use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use super::Session;
use super::visible_tab::visible_tab;

impl Session {
    /// The address of the tab this command would work in, read from the tab
    /// listing without claiming the tab, so a command that only needs it to
    /// resolve a relative address never waits on another command working
    /// there. `None` when the browser has no tab.
    pub(crate) fn current_url_unclaimed(&mut self) -> Result<Option<String>, AppError> {
        Ok(self.current_tab_unclaimed()?.map(|tab| tab.url))
    }

    /// The tab this command would work in, read from the tab listing
    /// without claiming it. `None` when the browser has no tab.
    pub(crate) fn current_tab_unclaimed(&mut self) -> Result<Option<PageTarget>, AppError> {
        let tabs = self.tabs()?;
        let chosen = match &self.scope.tab {
            Some(tab) => Some(tab.clone()),
            None => self.chosen_tab(&tabs)?,
        };
        Ok(chosen.and_then(|id| tabs.into_iter().find(|tab| tab.target_id == id)))
    }

    /// [`choose_tab`] among `tabs` with this command's `--tab` and the tab
    /// remembered from the last command, asking the tabs which one shows
    /// only when neither names one.
    pub(crate) fn chosen_tab(&mut self, tabs: &[PageTarget]) -> Result<Option<String>, AppError> {
        let remembered = self.store.read_state(&self.tab_state_name());
        let deadline = self.scope.deadline;
        let connection = &mut self.connection;
        choose_tab(
            tabs,
            self.scope.tab_hint.as_deref(),
            remembered.as_deref(),
            || visible_tab(connection, tabs, deadline),
        )
    }
}

/// Picks the tab a command works in. `hint` (`--tab`, or `tab <n>`) is an
/// index into `tabs` or a target id or unique id prefix; without one, the
/// tab the last command used when it is still open; otherwise, among more
/// than one tab, the one `visible` names (the tab the user is looking at,
/// asked only then), and the first tab when it names none of them. `None`
/// means the browser has no tab at all.
pub(crate) fn choose_tab(
    tabs: &[PageTarget],
    hint: Option<&str>,
    remembered: Option<&str>,
    visible: impl FnOnce() -> Option<String>,
) -> Result<Option<String>, AppError> {
    if let Some(hint) = hint {
        return find_tab(tabs, hint).map(Some);
    }
    if let Some(kept) = remembered.and_then(|id| tabs.iter().find(|tab| tab.target_id == id)) {
        return Ok(Some(kept.target_id.clone()));
    }
    let seen = if tabs.len() > 1 { visible() } else { None };
    let shown = seen.and_then(|id| tabs.iter().find(|tab| tab.target_id == id));
    Ok(shown
        .or_else(|| tabs.first())
        .map(|tab| tab.target_id.clone()))
}

pub(crate) fn find_tab(tabs: &[PageTarget], hint: &str) -> Result<String, AppError> {
    let hint = hint.trim();
    if let Ok(index) = hint.parse::<usize>() {
        return tabs
            .get(index)
            .map(|tab| tab.target_id.clone())
            .ok_or_else(|| no_such_tab(hint, tabs.len()));
    }
    let lowered = hint.to_ascii_lowercase();
    let matches: Vec<&PageTarget> = tabs
        .iter()
        .filter(|tab| tab.target_id.to_ascii_lowercase().starts_with(&lowered))
        .collect();
    match matches.as_slice() {
        [only] => Ok(only.target_id.clone()),
        [] => Err(no_such_tab(hint, tabs.len())),
        _ => Err(AppError::new(
            ErrorCode::AmbiguousTarget,
            format!("'{hint}' matches {} tabs", matches.len()),
        )
        .with_suggestion("Pass the tab's index from 'tabs' or more of its id")
        .with_disposition(DeliverySemantics::not_delivered())),
    }
}

fn no_such_tab(hint: &str, count: usize) -> AppError {
    AppError::new(
        ErrorCode::WindowNotFound,
        format!("No tab '{hint}' among {count} open tab(s)"),
    )
    .with_suggestion("Run 'tabref tabs' to list the open tabs")
    .with_details(serde_json::json!({ "kind": "tab_not_found" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "tab_choice_tests.rs"]
mod tests;
