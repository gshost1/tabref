use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::http_get::http_get_loopback;
use crate::{BrowserEndpoint, EndpointSource, WebSocketUrl};

/// Discovers the browser endpoint behind an explicit remote-debugging port by
/// reading `/json/version`. The advertised `webSocketDebuggerUrl` must be a
/// `ws://` loopback URL on that same port, so a responder that is not this
/// port's DevTools server is never mistaken for one.
pub fn endpoint_from_port(port: u16, deadline: Deadline) -> Result<BrowserEndpoint, AppError> {
    let body = http_get_loopback(port, "/json/version", deadline)?;
    parse_version_body(port, &body)
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
