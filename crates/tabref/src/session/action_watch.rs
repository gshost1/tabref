use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::{CdpEvent, wait_for_load};
use tabref_core::{AppError, Deadline};
use tabref_page::WatchedAction;

use super::Session;
use super::action_effects::ActionEffects;

const NEW_TAB_LOAD: Duration = Duration::from_millis(3_000);

impl Session {
    /// Starts watching `tab_id` for what the next action sets off: its
    /// navigations, requests, dialogs and DOM changes, and tabs it opens.
    pub(crate) fn watch_action(
        &mut self,
        tab_id: &str,
        deadline: Deadline,
    ) -> Result<WatchedAction, AppError> {
        let tab = self.attach(tab_id)?;
        let watched = WatchedAction::start(self.connection(), &tab, deadline)?;
        let connection = self.connection();
        connection.call(
            "Target.setDiscoverTargets",
            json!({ "discover": true }),
            None,
            deadline,
        )?;
        connection.discard_events(is_target_event);
        Ok(watched)
    }

    /// Waits for the watched action to settle and reports its effects. A
    /// tab the action opened becomes the current tab once it has loaded, or
    /// three seconds have passed.
    pub(crate) fn finish_action(
        &mut self,
        watched: &WatchedAction,
        deadline: Deadline,
    ) -> Result<ActionEffects, AppError> {
        let (settled, page) = watched.finish(self.connection(), deadline)?;
        let opener = watched.session.target_id.clone();
        let new_tab = self
            .connection()
            .drain_events(|event| is_page_opened_by(event, &opener))
            .last()
            .and_then(|event| event.params.pointer("/targetInfo/targetId"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.connection().discard_events(is_target_event);
        if let Some(tab) = &new_tab {
            self.use_tab(tab)?;
            let page = self.attach(tab)?;
            let _ = wait_for_load(self.connection(), &page, deadline.capped(NEW_TAB_LOAD));
        }
        Ok(ActionEffects {
            settled,
            new_tab,
            url_before: watched.mark.url.clone(),
            page,
        })
    }
}

fn is_target_event(event: &CdpEvent) -> bool {
    event.session_id.is_none() && event.method.starts_with("Target.target")
}

/// A page target the watched tab opened, as `window.open` or a
/// `target=_blank` link does: Chrome names its opener.
fn is_page_opened_by(event: &CdpEvent, opener: &str) -> bool {
    let info = |key: &str| {
        event
            .params
            .pointer(&format!("/targetInfo/{key}"))
            .and_then(Value::as_str)
    };
    event.method == "Target.targetCreated"
        && info("type") == Some("page")
        && info("openerId") == Some(opener)
}

#[cfg(test)]
#[path = "action_watch_tests.rs"]
mod tests;
