pub(crate) mod action_effects;
pub(crate) mod action_watch;
pub(crate) mod captured_snapshot;
pub(crate) mod command_input;
pub(crate) mod command_scope;
pub(crate) mod connected_browser;
pub(crate) mod connection_warning;
pub(crate) mod endpoint_choice;
pub(crate) mod endpoint_refusal;
pub(crate) mod kept_session;
pub(crate) mod launch_record;
pub(crate) mod ref_target;
pub(crate) mod session_options;
pub(crate) mod snapshot_capture;
pub(crate) mod tab_choice;
pub(crate) mod tab_claim;
pub(crate) mod tab_order;
pub(crate) mod user_chrome;
pub(crate) mod user_chrome_record;

use std::collections::HashMap;

use serde_json::json;
use tabref_cdp::{
    BrowserEndpoint, CdpConnection, PageSession, PageTarget, attach_to_target_with,
    create_background_tab, is_protocol_error, list_page_targets, target_gone,
};
use tabref_core::{AppError, Deadline, RefStore};

use command_scope::CommandScope;
use endpoint_choice::connect_browser;
use session_options::SessionOptions;
use tab_choice::choose_tab;
use tab_claim::TabClaim;
use tab_order::{order_text, stable_order};
use user_chrome_record::choice_in_use;

/// Everything one command runs against: the DevTools connection, the page
/// sessions attached through it, the ref store and what the current command
/// settled on. Commands never connect on their own. In-process, a `Session`
/// lives for one command; the daemon keeps one alive across commands and
/// starts a new [`CommandScope`] for each, so the connection, the attached
/// sessions and the domains they enabled are paid for once.
pub(crate) struct Session {
    connection: CdpConnection,
    endpoint: BrowserEndpoint,
    store: RefStore,
    attached: HashMap<String, PageSession>,
    scope: CommandScope,
    warning: Option<String>,
    choice: Option<String>,
}

impl Session {
    pub(crate) fn open(options: &SessionOptions) -> Result<Self, AppError> {
        Self::open_claimed(options, None)
    }

    /// [`open`](Self::open) for a daemon command, which holds `claim` on the
    /// tabs it works in.
    pub(crate) fn open_claimed(
        options: &SessionOptions,
        claim: Option<TabClaim>,
    ) -> Result<Self, AppError> {
        let scope = CommandScope::new(options, claim)?;
        let store = RefStore::open()?;
        let in_use = choice_in_use(&store, options);
        let record = in_use.as_ref().map(|(_, record)| record);
        let browser = connect_browser(options, &store, record, scope.deadline)?;
        let choice = in_use.map(|(text, _)| text);
        let mut connection = browser.connection;
        connection.set_cancel(scope.claim.as_ref().map(TabClaim::cancel_flag));
        Ok(Self {
            connection,
            endpoint: browser.endpoint,
            store,
            attached: HashMap::new(),
            scope,
            warning: browser.warning,
            choice,
        })
    }

    pub(crate) fn deadline(&self) -> Deadline {
        self.scope.deadline
    }

    pub(crate) fn connection(&mut self) -> &mut CdpConnection {
        &mut self.connection
    }

    pub(crate) fn store(&self) -> &RefStore {
        &self.store
    }

    /// The open tabs in a stable order: the order of the last listing, new
    /// tabs at the end. The order is saved, so `tab 1` and `close 1` name
    /// the tab the agent saw at index 1.
    pub(crate) fn tabs(&mut self) -> Result<Vec<PageTarget>, AppError> {
        let listed = list_page_targets(&mut self.connection, self.scope.deadline)?;
        let name = format!("tab-order-{}", self.endpoint.port());
        let saved = self.store.read_state(&name).unwrap_or_default();
        let tabs = stable_order(listed, &saved);
        let order = order_text(&tabs);
        if order != saved {
            self.store.write_state(&name, &order)?;
        }
        Ok(tabs)
    }

    /// The target id of the tab this command works in, chosen once.
    pub(crate) fn tab_id(&mut self) -> Result<String, AppError> {
        if let Some(tab) = &self.scope.tab {
            return Ok(tab.clone());
        }
        let tabs = self.tabs()?;
        let remembered = self.store.read_state(&self.tab_state_name());
        let chosen = choose_tab(&tabs, self.scope.tab_hint.as_deref(), remembered.as_deref())?;
        let tab = match chosen {
            Some(tab) => tab,
            None => {
                create_background_tab(&mut self.connection, "about:blank", self.scope.deadline)?
            }
        };
        self.use_tab(&tab)?;
        Ok(tab)
    }

    /// Makes `target_id` the tab this and later commands work in.
    pub(crate) fn use_tab(&mut self, target_id: &str) -> Result<(), AppError> {
        self.scope.claim(target_id)?;
        if remembered_is(&self.store, &self.tab_state_name(), target_id) {
            self.scope.tab = Some(target_id.to_owned());
            return Ok(());
        }
        self.store.write_state(&self.tab_state_name(), target_id)?;
        self.scope.tab = Some(target_id.to_owned());
        Ok(())
    }

    /// Forgets the current tab, after it was closed.
    pub(crate) fn forget_tab(&mut self) {
        self.scope.tab = None;
        self.store.remove_state(&self.tab_state_name());
    }

    /// The page session of the current tab.
    pub(crate) fn page(&mut self) -> Result<PageSession, AppError> {
        let tab = self.tab_id()?;
        self.attach(&tab)
    }

    /// The flattened session for `target_id`, attached once per connection,
    /// with page and lifecycle events on so navigations, loads and dialogs
    /// are seen. Turning them on rides in the attach's own round trip. A
    /// target the browser no longer has is `WINDOW_NOT_FOUND` with kind
    /// `cdp_target_gone`.
    pub(crate) fn attach(&mut self, target_id: &str) -> Result<PageSession, AppError> {
        self.scope.claim(target_id)?;
        if let Some(session) = self.attached.get(target_id) {
            return Ok(session.clone());
        }
        let deadline = self.scope.deadline;
        let events = [
            ("Page.enable", json!({})),
            ("Page.setLifecycleEventsEnabled", json!({ "enabled": true })),
        ];
        let session = attach_to_target_with(&mut self.connection, target_id, &events, deadline)
            .map_err(|error| {
                if is_protocol_error(&error) {
                    let detail = error.platform_detail.clone().unwrap_or_default();
                    target_gone(failed_method(&error), detail)
                } else {
                    error
                }
            })?;
        self.attached.insert(target_id.to_owned(), session.clone());
        Ok(session)
    }

    fn tab_state_name(&self) -> String {
        format!("tab-{}", self.endpoint.port())
    }
}

/// The method a protocol error names: attaching sends several, and the
/// error should say which one the browser refused.
fn failed_method(error: &AppError) -> &str {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("method"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Target.attachToTarget")
}

fn remembered_is(store: &RefStore, name: &str, target_id: &str) -> bool {
    store.read_state(name).as_deref() == Some(target_id)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
