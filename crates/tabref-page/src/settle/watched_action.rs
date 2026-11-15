use tabref_core::{AppError, Deadline};

use super::action_settle::{settle_action, watch_navigation};
use super::settle_outcome::SettleOutcome;
use crate::act::change_watch::{mark_page, page_changes};
use crate::{CdpConnection, PageChanges, PageMark, PageSession};

/// An action being watched: the page session it runs in, that page's main
/// frame, and the page as it stood before the action.
#[derive(Debug, Clone)]
pub struct WatchedAction {
    pub session: PageSession,
    pub main_frame: String,
    pub mark: PageMark,
}

impl WatchedAction {
    /// Starts watching `session` for what the next action sets off: page
    /// and network events, and the page's text and DOM mutations.
    pub fn start(
        connection: &mut CdpConnection,
        session: &PageSession,
        deadline: Deadline,
    ) -> Result<Self, AppError> {
        let main_frame = watch_navigation(connection, session, deadline)?;
        let mark = mark_page(connection, session, deadline)?;
        Ok(Self {
            session: session.clone(),
            main_frame,
            mark,
        })
    }

    /// Waits for the action to settle and reads what it changed. A page
    /// that cannot be read afterwards, such as one still being replaced,
    /// reports no changes rather than failing the action.
    pub fn finish(
        &self,
        connection: &mut CdpConnection,
        deadline: Deadline,
    ) -> Result<(SettleOutcome, Option<PageChanges>), AppError> {
        let settled = settle_action(connection, &self.session, &self.main_frame, deadline)?;
        let changes = page_changes(connection, &self.session, &self.mark, deadline).ok();
        Ok((settled, changes))
    }
}
