use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::{CdpEvent, is_protocol_error, is_target_gone, wait_for_load};
use tabref_core::{AppError, Deadline};
use tabref_page::{PageInfo, WatchedAction, follow_late_navigation};

use super::Session;
use super::action_effects::ActionEffects;
use crate::commands::downloads;

const NEW_TAB_LOAD: Duration = Duration::from_millis(3_000);
const LATE_WINDOW: Duration = Duration::from_millis(60);

impl Session {
    /// Starts watching `tab_id` for what the next action sets off: its
    /// navigations, requests, dialogs and DOM changes, tabs it opens, and
    /// downloads it starts.
    pub(crate) fn watch_action(
        &mut self,
        tab_id: &str,
        deadline: Deadline,
    ) -> Result<WatchedAction, AppError> {
        let tab = self.attach(tab_id)?;
        downloads::enable_for_action(self, deadline)?;
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
    /// three seconds have passed; one already gone again, as a popup that
    /// only downloads, is no new tab.
    pub(crate) fn finish_action(
        &mut self,
        watched: &WatchedAction,
        deadline: Deadline,
    ) -> Result<ActionEffects, AppError> {
        let (settled, page) = watched.finish(self.connection(), deadline)?;
        let opener = watched.session.target_id.clone();
        let opened = self
            .connection()
            .drain_events(|event| is_page_opened_by(event, &opener))
            .last()
            .and_then(|event| event.params.pointer("/targetInfo/targetId"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.connection().discard_events(is_target_event);
        let downloads = downloads::after_action(
            self,
            &watched.session,
            opened.as_deref(),
            deadline,
            Duration::ZERO,
        );
        let new_tab = match opened {
            Some(tab) => match self.attach(&tab) {
                Ok(page) => {
                    self.use_tab(&tab)?;
                    let _ = self.attach(&tab);
                    let _ = wait_for_load(self.connection(), &page, deadline.capped(NEW_TAB_LOAD));
                    Some(tab)
                }
                Err(error) if is_target_gone(&error) || is_protocol_error(&error) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
        Ok(ActionEffects {
            settled,
            new_tab,
            url_before: watched.mark.url.clone(),
            page,
            main_frame: watched.main_frame.clone(),
            downloads,
        })
    }
}

impl Session {
    /// After an action that set off nothing, waits up to 60 ms for a
    /// navigation it started late, from a timer or an async handler, and
    /// follows it, so the next step of a batch runs on the page the action
    /// led to. Returns that page when there was one.
    pub(crate) fn follow_late(
        &mut self,
        effects: &ActionEffects,
        deadline: Deadline,
    ) -> Result<Option<PageInfo>, AppError> {
        if effects.settled.navigated || effects.new_tab.is_some() || effects.main_frame.is_empty() {
            return Ok(None);
        }
        let page = self.page()?;
        let moved = follow_late_navigation(
            self.connection(),
            &page,
            &effects.main_frame,
            LATE_WINDOW,
            deadline,
        )?;
        if !moved {
            return Ok(None);
        }
        PageInfo::read(self.connection(), &page.target_id, deadline).map(Some)
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
