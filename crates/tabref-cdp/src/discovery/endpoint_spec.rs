use tabref_core::{AppError, Deadline};

use super::json_version::endpoint_from_port;
use crate::{BrowserEndpoint, EndpointSource, WebSocketUrl};

/// Where `--cdp` (or `TABREF_CDP`) says the browser listens: a port, an
/// `http://` DevTools address, or the browser's `ws://` debugger URL. Every
/// form must name a loopback host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointSpec {
    Port(u16),
    WebSocket(WebSocketUrl),
}

impl EndpointSpec {
    pub fn parse(text: &str) -> Result<Self, AppError> {
        let text = text.trim();
        if let Ok(port) = text.parse::<u16>() {
            return port_spec(port, text);
        }
        if text.starts_with("ws://") {
            return Ok(Self::WebSocket(WebSocketUrl::parse(text)?));
        }
        let Some(rest) = text.strip_prefix("http://") else {
            return Err(unrecognized(text));
        };
        let authority = rest.split('/').next().unwrap_or_default();
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or_else(|| unrecognized(text))?;
        if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
            return Err(AppError::invalid_args(format!(
                "Refusing DevTools address '{text}': the host is not a loopback address"
            ))
            .with_suggestion("tabref connects only to a browser on this machine"));
        }
        let port = port.parse::<u16>().map_err(|_| unrecognized(text))?;
        port_spec(port, text)
    }

    /// The browser endpoint this spec names. A port is asked for its
    /// `/json/version`, which costs one HTTP round trip; a WebSocket URL is
    /// taken as given.
    pub fn resolve(&self, deadline: Deadline) -> Result<BrowserEndpoint, AppError> {
        match self {
            Self::Port(port) => endpoint_from_port(*port, deadline),
            Self::WebSocket(url) => Ok(BrowserEndpoint {
                websocket_url: url.clone(),
                source: EndpointSource::WebSocketUrl,
                product: None,
            }),
        }
    }
}

fn port_spec(port: u16, text: &str) -> Result<EndpointSpec, AppError> {
    if port == 0 {
        return Err(unrecognized(text));
    }
    Ok(EndpointSpec::Port(port))
}

fn unrecognized(text: &str) -> AppError {
    AppError::invalid_args(format!("'{text}' is not a DevTools endpoint"))
        .with_suggestion(
            "Pass a port (9222), http://127.0.0.1:9222 or the browser's ws://127.0.0.1:9222/devtools/browser/<id> URL",
        )
        .with_details(serde_json::json!({ "kind": "cdp_endpoint_invalid" }))
}

#[cfg(test)]
#[path = "endpoint_spec_tests.rs"]
mod tests;
