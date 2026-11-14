use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::{CdpEvent, wait_for_load};
use tabref_core::AppError;
use tabref_page::{settle_action, watch_navigation};

use super::Session;
use super::action_effects::ActionEffects;

const NEW_TAB_LOAD: Duration = Duration::from_millis(3_000);

impl Session {
    /// Starts watching `tab_id` for what the next action sets off: its
    /// navigations and dialogs, and tabs it opens.
    pub(crate) fn watch_action(&mut self, tab_id: &str) -> Result<(), AppError> {
        let tab = self.attach(tab_id)?;
        let deadline = self.deadline();
        let connection = self.connection();
        watch_navigation(connection, &tab, deadline)?;
        connection.call(
            "Target.setDiscoverTargets",
            json!({ "discover": true }),
            None,
            deadline,
        )?;
        connection.discard_events(is_target_event);
        Ok(())
    }

    /// Waits for the watched action to settle and reports its effects. A
    /// tab the action opened becomes the current tab once it has loaded, or
    /// three seconds have passed.
    pub(crate) fn finish_action(&mut self) -> Result<ActionEffects, AppError> {
        let deadline = self.deadline();
        let settled = settle_action(self.connection(), deadline)?;
        let watched = self.tab_id()?;
        let new_tab = self
            .connection()
            .drain_events(|event| is_page_opened_by(event, &watched))
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
            navigated: settled.navigated,
            dialogs: settled.dialogs,
            new_tab,
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
