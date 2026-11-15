use crate::{EndpointSource, WebSocketUrl};

/// A browser-level DevTools WebSocket endpoint on loopback, with the
/// product and user agent `/json/version` named when it came from there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserEndpoint {
    pub websocket_url: WebSocketUrl,
    pub source: EndpointSource,
    pub product: Option<String>,
    pub user_agent: Option<String>,
}

impl BrowserEndpoint {
    pub fn port(&self) -> u16 {
        self.websocket_url.port()
    }
}
