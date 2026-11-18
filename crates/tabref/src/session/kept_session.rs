//! What the daemon needs of a session it keeps between commands: starting
//! and ending a command on it, and whether its connection still works.
#![cfg_attr(not(unix), allow(dead_code))]

use tabref_cdp::BrowserEndpoint;
use tabref_core::AppError;
use tabref_page::DIALOG_OPENING;

use super::Session;
use super::browser_choice::choice_key;
use super::command_scope::CommandScope;
use super::session_options::SessionOptions;
use super::tab_claim::TabClaim;

impl Session {
    /// Starts the next command on a kept session: a fresh deadline and tab
    /// choice, every frame the browser sent meanwhile accounted for, and the
    /// sessions of closed tabs forgotten. Events from before the command are
    /// dropped, as a new connection would never have seen them, except a
    /// dialog that opened since the last command: it is accepted (never with
    /// this command's `--dialog`, which is set only after it) and kept, so
    /// this command reports it as opened after the last command.
    /// An error means the connection is gone, or it is not to the browser this
    /// command wants (the user turned `connect --user-chrome` on or off, or
    /// one command asks for `--launch` and the next does not), and the
    /// session must be replaced.
    pub(crate) fn begin(
        &mut self,
        options: &SessionOptions,
        claim: Option<TabClaim>,
    ) -> Result<(), AppError> {
        let wanted = choice_key(&self.store, options);
        if wanted != self.choice {
            return Err(AppError::internal(
                "the browser choice changed since this connection was made",
            ));
        }
        self.scope = CommandScope::new(options, claim)?;
        let cancel = self.scope.claim.as_ref().map(TabClaim::cancel_flag);
        self.connection.set_cancel(cancel);
        self.connection.answer_next_dialog(None);
        self.connection.poll_pending(self.scope.deadline)?;
        self.connection
            .discard_events(|event| event.method != DIALOG_OPENING);
        self.connection.mark_buffered_dialogs_late();
        self.connection.answer_next_dialog(options.dialog.clone());
        let connection = &self.connection;
        self.attached
            .retain(|_, tab| !connection.is_session_detached(&tab.session.session_id));
        Ok(())
    }

    /// Ends the daemon command: its tab claims are released.
    pub(crate) fn end(&mut self) {
        self.scope.claim = None;
        self.connection.set_cancel(None);
    }

    /// Whether the connection failed, so no later command can use it.
    pub(crate) fn is_broken(&self) -> bool {
        self.connection.is_closed()
    }

    pub(crate) fn endpoint(&self) -> &BrowserEndpoint {
        &self.browser.endpoint
    }

    /// Keeps the user agent the browser gave, so a kept session asks once.
    pub(crate) fn remember_user_agent(&mut self, agent: &str) {
        self.browser.endpoint.user_agent = Some(agent.to_owned());
    }

    pub(crate) fn attached_count(&self) -> usize {
        self.attached.len()
    }
}
