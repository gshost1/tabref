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

    /// Whether this command holds `tab`, or can claim it without waiting.
    /// Outside the daemon every tab is this command's.
    pub(crate) fn try_claim(&self, tab: &str) -> bool {
        match (&self.claim, Deadline::after(0)) {
            (Some(claim), Ok(now)) => claim.claim(tab, now).is_ok(),
            (Some(_), Err(_)) => false,
            (None, _) => true,
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
