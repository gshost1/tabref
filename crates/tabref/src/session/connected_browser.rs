use tabref_cdp::{BrowserEndpoint, CdpConnection};

/// The browser a session connected to, the warning the command prints when
/// tabref could not vouch for it (an endpoint the user named whose port is
/// not held by a Chrome of this user), and the record that chose it, in the
/// form `browser_choice` compares (`None`: no record chose it).
pub(crate) struct ConnectedBrowser {
    pub(crate) endpoint: BrowserEndpoint,
    pub(crate) connection: CdpConnection,
    pub(crate) warning: Option<String>,
    pub(crate) choice: Option<String>,
    pub(crate) started_headless_profile: bool,
}
