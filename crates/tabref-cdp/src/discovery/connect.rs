use std::net::{Ipv4Addr, SocketAddr};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::connect_error::{approval_pending, connection_declined, endpoint_mismatch};
use super::held_port::HeldPort;
use crate::transport::transport_error::is_timeout;
use crate::{BrowserEndpoint, CdpConnection, EndpointSource, WebSocketTransport};

/// Opens a browser-level CDP connection. A browser reached through
/// `DevToolsActivePort` may hold the connection until its user approves it,
/// so that connection must answer `Browser.getVersion` first: this waits,
/// bounded by `deadline`, and reports a pending or declined approval as
/// `PERM_DENIED` with `details.kind` `cdp_approval_pending` or
/// `cdp_connection_rejected`. Any other endpoint already answered, or was
/// started by tabref, so it costs no extra round trip.
pub fn connect(endpoint: &BrowserEndpoint, deadline: Deadline) -> Result<CdpConnection, AppError> {
    connect_with(endpoint, deadline, &|| Ok(()))
}

/// [`connect`] to a port the owner check vouched for. The endpoint must be
/// `ws://127.0.0.1:<that port>` (kind `cdp_endpoint_mismatch` otherwise: an
/// address the check never looked at), and once the TCP connection is up,
/// before anything is sent, the port must still have the sockets the check
/// saw (kind `cdp_endpoint_changed` otherwise).
pub fn connect_held(
    endpoint: &BrowserEndpoint,
    held: &HeldPort,
    deadline: Deadline,
) -> Result<CdpConnection, AppError> {
    let checked = SocketAddr::from((Ipv4Addr::LOCALHOST, held.port));
    let advertised = endpoint.websocket_url.socket_addr();
    if advertised != checked {
        return Err(endpoint_mismatch(held.port, &advertised.to_string()));
    }
    connect_with(endpoint, deadline, &|| held.confirm())
}

fn connect_with(
    endpoint: &BrowserEndpoint,
    deadline: Deadline,
    before_upgrade: &dyn Fn() -> Result<(), AppError>,
) -> Result<CdpConnection, AppError> {
    let transport =
        WebSocketTransport::connect_checked(&endpoint.websocket_url, deadline, before_upgrade)
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
