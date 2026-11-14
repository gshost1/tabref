pub(crate) mod action_effects;
pub(crate) mod action_watch;
pub(crate) mod captured_snapshot;
pub(crate) mod endpoint_choice;
pub(crate) mod launch_record;
pub(crate) mod ref_target;
pub(crate) mod session_options;
pub(crate) mod snapshot_capture;
pub(crate) mod tab_choice;
pub(crate) mod tab_order;

use std::collections::HashMap;

use serde_json::json;
use tabref_cdp::{
    BrowserEndpoint, CdpConnection, PageSession, PageTarget, attach_to_target,
    create_background_tab, is_protocol_error, list_page_targets, target_gone,
};
use tabref_core::{AppError, Deadline, RefStore};

use endpoint_choice::connect_browser;
use session_options::SessionOptions;
use tab_choice::choose_tab;
use tab_order::{order_text, stable_order};

/// Everything one command runs against: the DevTools connection, the page
/// sessions attached through it, the tab in use and the ref store. Commands
/// never connect on their own; a later daemon keeps one `Session` alive
/// across commands instead of building one per invocation.
pub(crate) struct Session {
    connection: CdpConnection,
    endpoint: BrowserEndpoint,
    store: RefStore,
    attached: HashMap<String, PageSession>,
    tab: Option<String>,
    tab_hint: Option<String>,
    deadline: Deadline,
}

impl Session {
    pub(crate) fn open(options: &SessionOptions) -> Result<Self, AppError> {
        let deadline = Deadline::after(options.timeout_ms)?;
        let store = RefStore::open()?;
        let (endpoint, connection) = connect_browser(options, &store, deadline)?;
        Ok(Self {
            connection,
            endpoint,
            store,
            attached: HashMap::new(),
            tab: None,
            tab_hint: options.tab.clone(),
            deadline,
        })
    }

    pub(crate) fn deadline(&self) -> Deadline {
        self.deadline
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
        let listed = list_page_targets(&mut self.connection, self.deadline)?;
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
        if let Some(tab) = &self.tab {
            return Ok(tab.clone());
        }
        let tabs = self.tabs()?;
        let remembered = self.store.read_state(&self.tab_state_name());
        let chosen = choose_tab(&tabs, self.tab_hint.as_deref(), remembered.as_deref())?;
        let tab = match chosen {
            Some(tab) => tab,
            None => create_background_tab(&mut self.connection, "about:blank", self.deadline)?,
        };
        self.use_tab(&tab)?;
        Ok(tab)
    }

    /// Makes `target_id` the tab this and later commands work in.
    pub(crate) fn use_tab(&mut self, target_id: &str) -> Result<(), AppError> {
        if remembered_is(&self.store, &self.tab_state_name(), target_id) {
            self.tab = Some(target_id.to_owned());
            return Ok(());
        }
        self.store.write_state(&self.tab_state_name(), target_id)?;
        self.tab = Some(target_id.to_owned());
        Ok(())
    }

    /// Forgets the current tab, after it was closed.
    pub(crate) fn forget_tab(&mut self) {
        self.tab = None;
        self.store.remove_state(&self.tab_state_name());
    }

    /// The page session of the current tab.
    pub(crate) fn page(&mut self) -> Result<PageSession, AppError> {
        let tab = self.tab_id()?;
        self.attach(&tab)
    }

    /// The flattened session for `target_id`, attached once, with page
    /// events on so navigations and dialogs are seen. A target the browser no
    /// longer has is `WINDOW_NOT_FOUND` with kind `cdp_target_gone`.
    pub(crate) fn attach(&mut self, target_id: &str) -> Result<PageSession, AppError> {
        if let Some(session) = self.attached.get(target_id) {
            return Ok(session.clone());
        }
        let session =
            attach_to_target(&mut self.connection, target_id, self.deadline).map_err(|error| {
                if is_protocol_error(&error) {
                    let detail = error.platform_detail.clone().unwrap_or_default();
                    target_gone("Target.attachToTarget", detail)
                } else {
                    error
                }
            })?;
        self.connection.call(
            "Page.enable",
            json!({}),
            Some(&session.session_id),
            self.deadline,
        )?;
        self.attached.insert(target_id.to_owned(), session.clone());
        Ok(session)
    }

    fn tab_state_name(&self) -> String {
        format!("tab-{}", self.endpoint.port())
    }
}

fn remembered_is(store: &RefStore, name: &str, target_id: &str) -> bool {
    store.read_state(name).as_deref() == Some(target_id)
}
