use tabref_core::{AppError, Deadline};

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use super::json_version::endpoint_from_address;
use crate::{BrowserEndpoint, EndpointSource, WebSocketUrl};

/// Where `--cdp` (or `TABREF_CDP`) says the browser listens: a port, an
/// `http://` DevTools address, or the browser's `ws://` debugger URL. Every
/// form must name a loopback host. A bare port and `127.0.0.1` mean IPv4,
/// `[::1]` IPv6, and `localhost` tries IPv4 and then IPv6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointSpec {
    Port(u16),
    Ipv6Port(u16),
    Localhost(u16),
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
        let port = port.parse::<u16>().map_err(|_| unrecognized(text))?;
        let spec = port_spec(port, text)?;
        match host {
            "127.0.0.1" => Ok(spec),
            "[::1]" => Ok(Self::Ipv6Port(port)),
            "localhost" => Ok(Self::Localhost(port)),
            _ => Err(AppError::invalid_args(format!(
                "Refusing DevTools address '{text}': the host is not a loopback address"
            ))
            .with_suggestion("tabref connects only to a browser on this machine")),
        }
    }

    /// The browser endpoint this spec names. A port is asked for its
    /// `/json/version`, which costs one HTTP round trip; a WebSocket URL is
    /// taken as given.
    pub fn resolve(&self, deadline: Deadline) -> Result<BrowserEndpoint, AppError> {
        match self {
            Self::Port(port) => endpoint_from_address(ipv4(*port), deadline),
            Self::Ipv6Port(port) => endpoint_from_address(ipv6(*port), deadline),
            Self::Localhost(port) => endpoint_from_address(ipv4(*port), deadline)
                .or_else(|_| endpoint_from_address(ipv6(*port), deadline)),
            Self::WebSocket(url) => Ok(BrowserEndpoint {
                websocket_url: url.clone(),
                source: EndpointSource::WebSocketUrl,
                product: None,
            }),
        }
    }
}

fn ipv4(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

fn ipv6(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv6Addr::LOCALHOST, port))
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
