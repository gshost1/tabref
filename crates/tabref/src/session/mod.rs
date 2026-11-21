pub(crate) mod action_effects;
pub(crate) mod action_watch;
pub(crate) mod attached_tab;
pub(crate) mod browser_choice;
pub(crate) mod capture_cache;
pub(crate) mod captured_snapshot;
pub(crate) mod command_input;
pub(crate) mod command_scope;
pub(crate) mod connected_browser;
pub(crate) mod connection_warning;
pub(crate) mod endpoint_choice;
pub(crate) mod endpoint_refusal;
pub(crate) mod focus_state;
pub(crate) mod kept_session;
pub(crate) mod launch_check;
pub(crate) mod launch_profile;
pub(crate) mod launch_record;
pub(crate) mod reached_browser;
pub(crate) mod ref_target;
pub(crate) mod session_options;
pub(crate) mod snapshot_capture;
pub(crate) mod tab_choice;
pub(crate) mod tab_claim;
pub(crate) mod tab_order;
pub(crate) mod user_chrome;
pub(crate) mod user_chrome_record;
pub(crate) mod visible_tab;

use std::collections::HashMap;
use std::time::Duration;

use serde_json::json;
use tabref_cdp::{
    CdpConnection, PageSession, PageTarget, attach_to_target_with, create_background_tab,
    is_protocol_error, is_target_gone, list_page_targets, target_gone,
};
use tabref_core::{AppError, Deadline, RefStore};

use crate::commands::app_guard::{app_has_no_page, app_runtime};
use attached_tab::AttachedTab;
use capture_cache::CaptureAccess;
use command_scope::CommandScope;
use endpoint_choice::connect_browser;
use focus_state::FocusState;
use reached_browser::ReachedBrowser;
use session_options::SessionOptions;
use tab_claim::TabClaim;
use tab_order::{order_text, stable_order};
use user_chrome_record::{choice_in_use, opt_in_guard};

/// Everything one command runs against: the DevTools connection, the page
/// sessions attached through it, the ref store and what the current command
/// settled on. Commands never connect on their own. In-process, a `Session`
/// lives for one command; the daemon keeps one alive across commands and
/// starts a new [`CommandScope`] for each, so the connection, the attached
/// sessions and the domains they enabled are paid for once. A daemon command
/// that only reads may also keep captures; actions may reuse one to resolve
/// their first target, but never keep one across commands; batches cannot
/// reuse captures (see [`capture_cache::CaptureCache`]).
pub(crate) struct Session {
    connection: CdpConnection,
    browser: ReachedBrowser,
    store: RefStore,
    attached: HashMap<String, AttachedTab>,
    scope: CommandScope,
    warning: Option<String>,
    choice: Option<String>,
    captures: Option<CaptureAccess>,
    started_headless_profile: bool,
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
        let record = in_use
            .as_ref()
            .map(|(text, record)| (text.as_str(), record));
        let browser = connect_browser(options, &store, record, scope.deadline)?;
        let chosen = in_use.map(|(text, _)| text);
        let choice = browser.choice;
        let mut connection = browser.connection;
        connection.set_cancel(scope.claim.as_ref().map(TabClaim::cancel_flag));
        connection.set_guard(chosen.map(|chosen| opt_in_guard(&store, chosen)));
        connection.answer_next_dialog(options.dialog.clone());
        Ok(Self {
            connection,
            browser: ReachedBrowser::new(browser.endpoint, &store),
            store,
            attached: HashMap::new(),
            scope,
            warning: browser.warning,
            choice,
            captures: None,
            started_headless_profile: browser.started_headless_profile,
        })
    }

    pub(crate) fn take_headless_profile_launch(&mut self) -> bool {
        std::mem::take(&mut self.started_headless_profile)
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
        let name = format!("tab-order-{}", self.browser.endpoint.port());
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
        let tab = match self.chosen_tab(&tabs)? {
            Some(tab) => tab,
            None => {
                let deadline = self.scope.deadline;
                if let Some(runtime) = app_runtime(self, deadline)? {
                    return Err(app_has_no_page(&runtime));
                }
                create_background_tab(&mut self.connection, "about:blank", deadline)?
            }
        };
        self.use_tab(&tab)?;
        Ok(tab)
    }

    /// Makes `target_id` the tab this and later commands work in. A tab
    /// tabref worked in before stops being told it holds the window focus.
    pub(crate) fn use_tab(&mut self, target_id: &str) -> Result<(), AppError> {
        self.scope.claim(target_id)?;
        let leaving = self.scope.deadline.capped(Duration::from_millis(LEAVE_MS));
        self.leave_other_tabs(Some(target_id), leaving);
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
    /// `cdp_target_gone`. In a browser tabref launched, the current tab is
    /// also told it holds the window focus (`OWN_TAB_FOCUS`), once per
    /// session: when it is attached, or attached again once it is current.
    pub(crate) fn attach(&mut self, target_id: &str) -> Result<PageSession, AppError> {
        self.scope.claim(target_id)?;
        let deadline = self.scope.deadline;
        let focus = self.browser.launched && self.scope.tab.as_deref() == Some(target_id);
        if let Some(tab) = self.attached.get_mut(target_id) {
            if focus && tab.focus != FocusState::On {
                let told = self.connection.call(
                    OWN_TAB_FOCUS,
                    json!({ "enabled": true }),
                    Some(&tab.session.session_id),
                    deadline,
                );
                if let Err(error) = told {
                    tab.focus = FocusState::Unknown;
                    return Err(attach_error(error));
                }
                tab.focus = FocusState::On;
            }
            return Ok(tab.session.clone());
        }
        let mut events = vec![
            ("Page.enable", json!({})),
            ("Page.setLifecycleEventsEnabled", json!({ "enabled": true })),
        ];
        if focus {
            events.push((OWN_TAB_FOCUS, json!({ "enabled": true })));
        }
        let session = attach_to_target_with(&mut self.connection, target_id, &events, deadline)
            .map_err(attach_error)?;
        let tab = AttachedTab {
            session: session.clone(),
            focus: if focus {
                FocusState::On
            } else {
                FocusState::Off
            },
        };
        self.attached.insert(target_id.to_owned(), tab);
        Ok(session)
    }

    /// Whether tabref launched this browser, as decided when the session
    /// opened: its tabs are tabref's own and headless, so tabref may tell
    /// the tab it works in that it holds the window focus.
    pub(crate) fn launched_by_tabref(&self) -> bool {
        self.browser.launched
    }

    /// Stops telling each tab but `keep` that it holds the window focus,
    /// so a tab tabref no longer works in goes back to the background:
    /// hidden, with throttled timers. Only a tab no other daemon command
    /// holds is told, held for that one call and released at once; a tab
    /// another command works in now keeps its state, and its flag. A
    /// dialog the tab opens while it is left (a blur handler's `confirm`)
    /// is accepted, and the command's own `--dialog` answer is kept for the
    /// command's own dialog. A tab is `Off` once the browser took the call
    /// or the tab is gone; a call that failed otherwise (a cancel, the
    /// deadline) leaves it `Unknown`, so the next switch-off tries again and
    /// the next attach while it is current tells it again.
    fn leave_other_tabs(&mut self, keep: Option<&str>, deadline: Deadline) {
        let choice = self.connection.take_dialog_choice();
        for (id, tab) in &mut self.attached {
            if tab.focus == FocusState::Off || keep == Some(id.as_str()) {
                continue;
            }
            let Some(taken) = self.scope.try_hold(id) else {
                continue;
            };
            let left = self.connection.call(
                OWN_TAB_FOCUS,
                json!({ "enabled": false }),
                Some(&tab.session.session_id),
                deadline,
            );
            if taken {
                self.scope.release_tab(id);
            }
            tab.focus = if left.as_ref().map_or_else(is_target_gone, |_| true) {
                FocusState::Off
            } else {
                FocusState::Unknown
            };
        }
        self.connection.answer_next_dialog(choice);
    }

    /// The tabs this connection told they hold the window focus, or may
    /// have.
    pub(crate) fn emulated_tabs(&self) -> Vec<String> {
        self.attached
            .iter()
            .filter(|(_, tab)| tab.focus != FocusState::Off)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// For a daemon command that ends, while it still holds its claim:
    /// every tab this connection told it holds the window focus, other than
    /// the tab later commands start in, is told so no longer, so a
    /// connection that waits in the pool leaves no background tab visible.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub(crate) fn leave_background_tabs(&mut self) {
        if !self.browser.launched
            || self
                .attached
                .values()
                .all(|tab| tab.focus == FocusState::Off)
        {
            return;
        }
        let current = self.store.read_state(&self.tab_state_name());
        if let Ok(deadline) = Deadline::after(LEAVE_MS) {
            self.leave_other_tabs(current.as_deref().map(str::trim), deadline);
        }
    }

    fn tab_state_name(&self) -> String {
        format!("tab-{}", self.browser.endpoint.port())
    }
}

/// The attach hook for the tab tabref works in, in a browser tabref
/// launched: the tab reads as holding the window focus while it is the
/// current tab, so its focus handlers run and every action sees the same
/// focus state, and is told otherwise when tabref moves to another tab
/// (`leave_other_tabs`). Other tabs keep their own state, and a browser
/// tabref did not launch is never told this. The daemon's pooled
/// connections each keep their own record: a tab skipped because another
/// command held it, or one another connection turned off, may stay told,
/// or lose it, until a command next makes it current.
const OWN_TAB_FOCUS: &str = "Emulation.setFocusEmulationEnabled";

/// How long a command spends telling the tabs it leaves, on a tab change
/// or when it ends, that they no longer hold the window focus: a page that
/// stalls in its blur handler must not hold the command up.
const LEAVE_MS: u64 = 300;

/// An attach the browser refused: the target is gone.
fn attach_error(error: AppError) -> AppError {
    if is_protocol_error(&error) {
        let detail = error.platform_detail.clone().unwrap_or_default();
        target_gone(failed_method(&error), detail)
    } else {
        error
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
