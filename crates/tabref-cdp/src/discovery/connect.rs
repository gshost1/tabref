use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::connect_error::{approval_pending, connection_declined};
use crate::transport::stalled_retry::retry_once_if_stalled;
use crate::transport::transport_error::is_timeout;
use crate::{BrowserEndpoint, CdpConnection, EndpointSource, WebSocketTransport};

/// Opens a browser-level CDP connection. A browser reached through
/// `DevToolsActivePort` may hold the connection until its user approves it,
/// so that connection must answer `Browser.getVersion` first: this waits,
/// bounded by `deadline`, and reports a pending or declined approval as
/// `PERM_DENIED` with `details.kind` `cdp_approval_pending` or
/// `cdp_connection_rejected`. Any other endpoint already answered, or was
/// started by tabref, so it costs no extra round trip. Any other endpoint
/// whose upgrade stalls is retried once on a fresh connection (see
/// `stalled_retry`); a held `DevToolsActivePort` upgrade is the browser
/// asking its user, so it gets one attempt and is never asked twice.
pub fn connect(endpoint: &BrowserEndpoint, deadline: Deadline) -> Result<CdpConnection, AppError> {
    let url = &endpoint.websocket_url;
    let transport = if endpoint.source == EndpointSource::DevToolsActivePort {
        WebSocketTransport::connect(url, deadline)
    } else {
        retry_once_if_stalled(deadline, |attempt| {
            WebSocketTransport::connect(url, attempt)
        })
    }
    .map_err(|error| classify(endpoint, deadline, error))?;
    let mut connection = CdpConnection::new(Box::new(transport));
    if endpoint.source == EndpointSource::DevToolsActivePort {
        verify_connection(&mut connection, endpoint, deadline)?;
    }
    Ok(connection)
}

pub(crate) fn verify_connection(
    connection: &mut CdpConnection,
    endpoint: &BrowserEndpoint,
    deadline: Deadline,
) -> Result<Value, AppError> {
    connection
        .call("Browser.getVersion", json!({}), None, deadline)
        .map_err(|error| classify(endpoint, deadline, error))
}

fn classify(endpoint: &BrowserEndpoint, deadline: Deadline, error: AppError) -> AppError {
    if endpoint.source != EndpointSource::DevToolsActivePort {
        return error;
    }
    if is_timeout(&error) {
        return approval_pending(endpoint.port(), deadline.elapsed().as_millis());
    }
    let closed = error
        .details
        .as_ref()
        .and_then(|details| details.get("kind"))
        .and_then(Value::as_str)
        == Some("cdp_transport_closed");
    if closed {
        let detail = error.platform_detail.clone().unwrap_or_default();
        return connection_declined(endpoint.port(), &detail);
    }
    error
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod tests;
