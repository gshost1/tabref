use tabref_core::{AppError, Deadline};

use super::session_options::SessionOptions;
use super::tab_claim::TabClaim;

/// What a session knows only for the command it runs now: the deadline, the
/// tab the command settled on, the tab `--tab` asked for, and, inside the
/// daemon, the command's claim on the tabs it touches and the standard
/// input its client sent. A daemon session
/// starts a new scope for every command it serves.
#[derive(Debug)]
pub(crate) struct CommandScope {
    pub(crate) deadline: Deadline,
    pub(crate) tab: Option<String>,
    pub(crate) tab_hint: Option<String>,
    pub(crate) claim: Option<TabClaim>,
    pub(crate) stdin: Option<String>,
}

impl CommandScope {
    pub(crate) fn new(options: &SessionOptions, claim: Option<TabClaim>) -> Result<Self, AppError> {
        Ok(Self {
            deadline: Deadline::after(options.timeout_ms)?,
            tab: None,
            tab_hint: options.tab.clone(),
            claim,
            stdin: options.stdin.clone(),
        })
    }

    /// Holds `tab` for one call if no other daemon command holds it, without
    /// waiting: `Some(true)` when taken now (release it with
    /// [`release_tab`](Self::release_tab) after the call), `Some(false)`
    /// when this command held it already or runs outside the daemon,
    /// `None` when another command holds it.
    pub(crate) fn try_hold(&self, tab: &str) -> Option<bool> {
        match &self.claim {
            Some(claim) => claim.try_hold(tab),
            None => Some(false),
        }
    }

    /// Releases a hold [`try_hold`](Self::try_hold) took.
    pub(crate) fn release_tab(&self, tab: &str) {
        if let Some(claim) = &self.claim {
            claim.release_tab(tab);
        }
    }

    /// Claims `tab` for this command when it runs in the daemon.
    pub(crate) fn claim(&self, tab: &str) -> Result<(), AppError> {
        match &self.claim {
            Some(claim) => claim.claim(tab, self.deadline),
            None => Ok(()),
        }
    }
}
