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

    /// Claims `tab` for this command when it runs in the daemon.
    pub(crate) fn claim(&self, tab: &str) -> Result<(), AppError> {
        match &self.claim {
            Some(claim) => claim.claim(tab, self.deadline),
            None => Ok(()),
        }
    }
}
