use serde_json::Value;
use tabref_page::{PageChanges, SettleOutcome};

/// What an action set off: how its tab settled (a new document, a route
/// change, requests, the dialogs it opened and the connection accepted),
/// the tab it opened, which is now the current one, the address before the
/// action, and the page's changes against how it stood before, when the
/// page could be read afterwards.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ActionEffects {
    pub(crate) settled: SettleOutcome,
    pub(crate) new_tab: Option<String>,
    pub(crate) url_before: String,
    pub(crate) page: Option<PageChanges>,
}

impl ActionEffects {
    pub(crate) fn dialogs(&self) -> &[Value] {
        &self.settled.dialogs
    }

    /// The address after the action, when it differs from the one before.
    pub(crate) fn moved_to(&self) -> Option<&str> {
        self.page
            .as_ref()
            .map(|page| page.url.as_str())
            .filter(|url| !url.is_empty() && *url != self.url_before)
    }
}
