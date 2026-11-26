use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::connect_error::{approval_pending, connection_declined, endpoint_mismatch};
use super::held_port::HeldPort;
use crate::transport::transport_error::is_timeout;
use crate::transport::websocket::{PendingUpgrade, Upgrade};
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

/// [`connect`], running `before_upgrade` once the TCP connection is up and
/// before anything is sent on it, for a caller with its own check that the
/// port did not change hands (macOS: `lsof`).
pub fn connect_with(
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

/// How a connection to a browser that asks its user to approve each
/// DevTools connection stood at the deadline.
pub enum Approval {
    Approved(CdpConnection),
    /// Still waiting for the user's answer; the connection is kept open, so
    /// the prompt the browser shows for it stays the one that counts.
    Pending(PendingApproval),
}

/// A connection the browser has not approved yet, kept open so it can be
/// waited on again without opening another one (each new connection makes
/// the browser show one more prompt).
pub struct PendingApproval {
    stage: PendingStage,
    endpoint: BrowserEndpoint,
    since: Instant,
}

enum PendingStage {
    /// The browser holds the WebSocket upgrade.
    Upgrade(PendingUpgrade),
    /// The upgrade went through and `Browser.getVersion` is not answered.
    Verify(Box<CdpConnection>),
}

impl PendingApproval {
    /// Waits for the user's answer until `deadline`, on the same connection.
    pub fn resume(self, deadline: Deadline) -> Result<Approval, AppError> {
        let Self {
            stage,
            endpoint,
            since,
        } = self;
        let connection = match stage {
            PendingStage::Upgrade(upgrade) => {
                match upgrade
                    .resume(deadline)
                    .map_err(|error| classify(&endpoint, deadline, error))?
                {
                    Upgrade::Done(transport) => CdpConnection::new(Box::new(transport)),
                    Upgrade::Held(upgrade) => {
                        return Ok(Approval::Pending(Self {
                            stage: PendingStage::Upgrade(upgrade),
                            endpoint,
                            since,
                        }));
                    }
                }
            }
            PendingStage::Verify(connection) => *connection,
        };
        verified(connection, endpoint, deadline, since)
    }

    /// How long the connection has waited for approval.
    pub fn waited(&self) -> Duration {
        self.since.elapsed()
    }

    pub fn port(&self) -> u16 {
        self.endpoint.port()
    }
}

/// [`connect_with`] that keeps a connection the browser has not approved
/// by `deadline` instead of dropping it: see [`Approval`]. An endpoint not
/// read from `DevToolsActivePort` never asks, and is never pending.
pub fn connect_awaiting(
    endpoint: &BrowserEndpoint,
    deadline: Deadline,
    before_upgrade: &dyn Fn() -> Result<(), AppError>,
) -> Result<Approval, AppError> {
    if endpoint.source != EndpointSource::DevToolsActivePort {
        return connect_with(endpoint, deadline, before_upgrade).map(Approval::Approved);
    }
    let since = Instant::now();
    let upgrade =
        WebSocketTransport::start_checked(&endpoint.websocket_url, deadline, before_upgrade)
            .map_err(|error| classify(endpoint, deadline, error))?;
    match upgrade {
        Upgrade::Done(transport) => verified(
            CdpConnection::new(Box::new(transport)),
            endpoint.clone(),
            deadline,
            since,
        ),
        Upgrade::Held(upgrade) => Ok(Approval::Pending(PendingApproval {
            stage: PendingStage::Upgrade(upgrade),
            endpoint: endpoint.clone(),
            since,
        })),
    }
}

/// [`connect_held`] that keeps a connection still waiting for approval:
/// see [`connect_awaiting`].
pub fn connect_held_awaiting(
    endpoint: &BrowserEndpoint,
    held: &HeldPort,
    deadline: Deadline,
) -> Result<Approval, AppError> {
    let checked = SocketAddr::from((Ipv4Addr::LOCALHOST, held.port));
    let advertised = endpoint.websocket_url.socket_addr();
    if advertised != checked {
        return Err(endpoint_mismatch(held.port, &advertised.to_string()));
    }
    connect_awaiting(endpoint, deadline, &|| held.confirm())
}

/// `connection` once the browser answers on it; still pending when the
/// deadline passes first.
fn verified(
    mut connection: CdpConnection,
    endpoint: BrowserEndpoint,
    deadline: Deadline,
    since: Instant,
) -> Result<Approval, AppError> {
    match verify_connection(&mut connection, &endpoint, deadline) {
        Ok(_) => Ok(Approval::Approved(connection)),
        Err(error) if error.kind() == Some("cdp_approval_pending") => {
            Ok(Approval::Pending(PendingApproval {
                stage: PendingStage::Verify(Box::new(connection)),
                endpoint,
                since,
            }))
        }
        Err(error) => Err(error),
    }
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
