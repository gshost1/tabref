use tabref_cdp::{BrowserEndpoint, EndpointSource, LaunchedBrowser};
use tabref_core::RefStore;

use super::launch_check::{LaunchCheck, launched_check};
use super::launch_record::load_launched;

/// The browser a [`Session`](super::Session) reached, and whether tabref
/// launched it, decided once when the session opens.
#[derive(Debug)]
pub(crate) struct ReachedBrowser {
    pub(crate) endpoint: BrowserEndpoint,
    pub(crate) launched: bool,
}

impl ReachedBrowser {
    /// `endpoint`, launched by tabref when it was reached through the
    /// launch record (whose listener was checked then), or when the record
    /// names it (see [`names_endpoint`]). A record whose browser died, or
    /// whose port another browser took, does not count.
    pub(crate) fn new(endpoint: BrowserEndpoint, store: &RefStore) -> Self {
        let launched = endpoint.source == EndpointSource::Launched
            || load_launched(store)
                .is_some_and(|record| names_endpoint(&record, &endpoint, launched_check));
        Self { endpoint, launched }
    }
}

/// Whether `record` names `endpoint`, a loopback endpoint reached another
/// way (such as `--cdp`): on the record's port, and the browser listening
/// there is the one the record names, as `check` says (its process runs the
/// record's profile, see `launch_check`).
pub(crate) fn names_endpoint(
    record: &LaunchedBrowser,
    endpoint: &BrowserEndpoint,
    check: impl FnOnce(&LaunchedBrowser) -> LaunchCheck,
) -> bool {
    endpoint.websocket_url.socket_addr().ip().is_loopback()
        && record.port == endpoint.port()
        && check(record).is_launched()
}

#[cfg(test)]
#[path = "reached_browser_tests.rs"]
mod tests;
