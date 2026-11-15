//! What the daemon needs of a session it keeps between commands: starting
//! and ending a command on it, and whether its connection still works.
#![cfg_attr(not(unix), allow(dead_code))]

use tabref_cdp::BrowserEndpoint;
use tabref_core::AppError;

use super::Session;
use super::command_scope::CommandScope;
use super::session_options::SessionOptions;
use super::tab_claim::TabClaim;

impl Session {
    /// Starts the next command on a kept session: a fresh deadline and tab
    /// choice, every frame the browser sent meanwhile accounted for, and the
    /// sessions of closed tabs forgotten. Events from before the command are
    /// dropped, as a new connection would never have seen them. An error
    /// means the connection is gone and the session must be replaced.
    pub(crate) fn begin(
        &mut self,
        options: &SessionOptions,
        claim: Option<TabClaim>,
    ) -> Result<(), AppError> {
        self.scope = CommandScope::new(options, claim)?;
        let cancel = self.scope.claim.as_ref().map(TabClaim::cancel_flag);
        self.connection.set_cancel(cancel);
        self.connection.poll_pending(self.scope.deadline)?;
        self.connection.take_events();
        let connection = &self.connection;
        self.attached
            .retain(|_, session| !connection.is_session_detached(&session.session_id));
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
        &self.endpoint
    }

    pub(crate) fn attached_count(&self) -> usize {
        self.attached.len()
    }
}
