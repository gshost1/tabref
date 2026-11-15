use tabref_cdp::{BrowserEndpoint, CdpConnection};

/// The browser a session connected to, and the warning the command prints
/// when tabref could not vouch for it (an endpoint the user named whose port
/// is not held by a Chrome of this user).
pub(crate) struct ConnectedBrowser {
    pub(crate) endpoint: BrowserEndpoint,
    pub(crate) connection: CdpConnection,
    pub(crate) warning: Option<String>,
}
