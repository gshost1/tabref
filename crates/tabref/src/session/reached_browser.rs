use tabref_cdp::{BrowserEndpoint, EndpointSource};
use tabref_core::RefStore;

use super::endpoint_choice::launched_check;
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
    /// launch record (whose listener was checked then), or when it is a
    /// loopback endpoint on the record's port and the browser listening
    /// there is the one the record names: its process runs the record's
    /// profile. A record whose browser died, or whose port another browser
    /// took, does not count.
    pub(crate) fn new(endpoint: BrowserEndpoint, store: &RefStore) -> Self {
        let launched = endpoint.source == EndpointSource::Launched
            || (endpoint.websocket_url.socket_addr().ip().is_loopback()
                && load_launched(store).is_some_and(|record| {
                    record.port == endpoint.port() && launched_check(&record).is_some()
                }));
        Self { endpoint, launched }
    }
}
