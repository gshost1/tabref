use std::net::SocketAddr;

use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::held_port::HeldPort;
use super::http_get::{http_get_checked, http_get_loopback};
use crate::{BrowserEndpoint, EndpointSource, WebSocketUrl};

/// Discovers the browser endpoint behind an explicit remote-debugging port by
/// reading `/json/version`. The advertised `webSocketDebuggerUrl` must be a
/// `ws://` loopback URL on that same port, so a responder that is not this
/// port's DevTools server is never mistaken for one.
pub fn endpoint_from_port(port: u16, deadline: Deadline) -> Result<BrowserEndpoint, AppError> {
    endpoint_from_address(SocketAddr::from(([127, 0, 0, 1], port)), deadline)
}

/// [`endpoint_from_port`] for a DevTools server on a given loopback
/// address, such as `[::1]:9222`.
pub fn endpoint_from_address(
    address: SocketAddr,
    deadline: Deadline,
) -> Result<BrowserEndpoint, AppError> {
    http_get_loopback(address, "/json/version", deadline)
        .and_then(|body| parse_version_body(address.port(), &body))
        .map_err(|error| discovery_hint(address, error))
}

fn discovery_hint(address: SocketAddr, mut error: AppError) -> AppError {
    match error.kind() {
        Some("cdp_transport_failed") => {
            error.message =
                format!("Could not read a DevTools endpoint from {address}/json/version");
        }
        Some("cdp_timeout") => {
            error.message = format!("DevTools discovery at {address}/json/version timed out");
        }
        Some("cdp_endpoint_invalid") => {}
        _ => return error,
    }
    error.with_suggestion(format!(
        "If the browser provides a debugger WebSocket URL, pass its full URL directly, \
         for example --cdp ws://{address}/devtools/browser/<id>"
    ))
}

/// [`endpoint_from_port`] for a port the owner check vouched for: the probe
/// goes to `127.0.0.1:<port>` and, once its TCP connection is up, is written
/// only while the port still has the sockets the check saw (kind
/// `cdp_endpoint_changed` otherwise, with nothing sent).
pub fn endpoint_from_held(
    held: &HeldPort,
    deadline: Deadline,
) -> Result<BrowserEndpoint, AppError> {
    let address = SocketAddr::from(([127, 0, 0, 1], held.port));
    let body = http_get_checked(address, "/json/version", deadline, &|| held.confirm())?;
    parse_version_body(held.port, &body)
}

pub(crate) fn parse_version_body(port: u16, body: &str) -> Result<BrowserEndpoint, AppError> {
    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|_| not_devtools(port))?;
    let websocket_url = parsed
        .get("webSocketDebuggerUrl")
        .and_then(serde_json::Value::as_str)
        .and_then(|url| WebSocketUrl::parse(url).ok())
        .filter(|url| url.port() == port)
        .ok_or_else(|| not_devtools(port))?;
    Ok(BrowserEndpoint {
        websocket_url,
        source: EndpointSource::JsonVersion,
        product: parsed
            .get("Browser")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        user_agent: parsed
            .get("User-Agent")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

fn not_devtools(port: u16) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "The server on port {port} did not answer /json/version as a loopback DevTools endpoint"
        ),
    )
    .with_details(serde_json::json!({ "kind": "cdp_endpoint_invalid", "port": port }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "json_version_tests.rs"]
mod tests;
