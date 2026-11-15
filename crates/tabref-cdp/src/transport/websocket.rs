use std::io::ErrorKind;
use std::net::TcpStream;
use std::time::Duration;

use tabref_core::{AppError, Deadline};
use tungstenite::client::IntoClientRequest;
use tungstenite::handshake::HandshakeError;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Error as WsError, Message, WebSocket};

use super::cdp_transport::CdpTransport;
use super::transport_error::{transport_closed, transport_failed, transport_timeout};
use crate::WebSocketUrl;
use crate::discovery::connect_error::handshake_rejected;

const MAX_MESSAGE_BYTES: usize = 256 * 1024 * 1024;
const MIN_IO_SLICE: Duration = Duration::from_millis(1);

/// A blocking WebSocket client to a loopback DevTools server. Large
/// messages such as a full accessibility tree are accepted up to 256 MiB.
#[derive(Debug)]
pub struct WebSocketTransport {
    socket: WebSocket<TcpStream>,
}

impl WebSocketTransport {
    /// Opens the TCP connection and completes the WebSocket upgrade before
    /// `deadline`. A server that holds the upgrade (a browser waiting on its
    /// user's approval) costs at most the remaining deadline.
    pub fn connect(url: &WebSocketUrl, deadline: Deadline) -> Result<Self, AppError> {
        Self::connect_checked(url, deadline, &|| Ok(()))
    }

    /// [`connect`](Self::connect), running `before_upgrade` once the TCP
    /// connection is up and before a single byte is sent on it; its error
    /// drops the connection unused.
    pub fn connect_checked(
        url: &WebSocketUrl,
        deadline: Deadline,
        before_upgrade: &dyn Fn() -> Result<(), AppError>,
    ) -> Result<Self, AppError> {
        let stream = TcpStream::connect_timeout(&url.socket_addr(), io_slice(deadline)?)
            .map_err(|error| connect_failed(url, &error))?;
        before_upgrade()?;
        stream
            .set_nodelay(true)
            .map_err(|error| transport_failed("connect", error.to_string()))?;
        apply_timeouts(&stream, deadline)?;
        let request = url
            .as_url()
            .into_client_request()
            .map_err(|error| transport_failed("handshake", error.to_string()))?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES));
        let mut pending = tungstenite::client::client_with_config(request, stream, Some(config));
        loop {
            match pending {
                Ok((socket, _)) => return Ok(Self { socket }),
                Err(HandshakeError::Interrupted(mut mid)) => {
                    if deadline.is_expired() {
                        return Err(transport_timeout("handshake"));
                    }
                    apply_timeouts(mid.get_mut().get_mut(), deadline)?;
                    pending = mid.handshake();
                }
                Err(HandshakeError::Failure(WsError::Http(response))) => {
                    return Err(handshake_rejected(response.status().as_u16(), url.port()));
                }
                Err(HandshakeError::Failure(error)) => {
                    return Err(map_ws_error("handshake", error));
                }
            }
        }
    }
}

impl CdpTransport for WebSocketTransport {
    fn send_text(&mut self, text: &str, deadline: Deadline) -> Result<(), AppError> {
        apply_timeouts(self.socket.get_ref(), deadline)?;
        self.socket
            .send(Message::text(text))
            .map_err(|error| map_ws_error("send", error))
    }

    fn receive_text(&mut self, deadline: Deadline) -> Result<String, AppError> {
        loop {
            apply_timeouts(self.socket.get_ref(), deadline)?;
            match self.socket.read() {
                Ok(message) => {
                    if let Some(text) = message_text(message, "receive") {
                        return text;
                    }
                }
                Err(WsError::Io(error)) if is_retry(error.kind()) => {
                    if deadline.is_expired() {
                        return Err(transport_timeout("receive"));
                    }
                }
                Err(error) => return Err(map_ws_error("receive", error)),
            }
        }
    }

    fn poll_text(&mut self) -> Result<Option<String>, AppError> {
        set_nonblocking(self.socket.get_ref(), true)?;
        let polled = loop {
            match self.socket.read() {
                Ok(message) => {
                    if let Some(text) = message_text(message, "poll") {
                        break text.map(Some);
                    }
                }
                Err(WsError::Io(error)) if is_retry(error.kind()) => break Ok(None),
                Err(error) => break Err(map_ws_error("poll", error)),
            }
        };
        set_nonblocking(self.socket.get_ref(), false)?;
        polled
    }
}

/// A data message's text, or the error a close frame means. `None` for
/// control frames, which tungstenite answers itself.
fn message_text(message: Message, operation: &str) -> Option<Result<String, AppError>> {
    match message {
        Message::Text(text) => Some(Ok(text.as_str().to_owned())),
        Message::Binary(bytes) => Some(
            String::from_utf8(bytes.to_vec())
                .map_err(|error| transport_failed(operation, error.to_string())),
        ),
        Message::Close(frame) => Some(Err(transport_closed(
            frame.map_or_else(String::new, |frame| frame.reason.as_str().to_owned()),
        ))),
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => None,
    }
}

fn set_nonblocking(stream: &TcpStream, nonblocking: bool) -> Result<(), AppError> {
    stream
        .set_nonblocking(nonblocking)
        .map_err(|error| transport_failed("configure", error.to_string()))
}

fn is_retry(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}

fn io_slice(deadline: Deadline) -> Result<Duration, AppError> {
    if deadline.is_expired() {
        return Err(transport_timeout("connect"));
    }
    Ok(deadline.remaining().max(MIN_IO_SLICE))
}

fn apply_timeouts(stream: &TcpStream, deadline: Deadline) -> Result<(), AppError> {
    let slice = Some(deadline.remaining().max(MIN_IO_SLICE));
    stream
        .set_read_timeout(slice)
        .and_then(|()| stream.set_write_timeout(slice))
        .map_err(|error| transport_failed("configure", error.to_string()))
}

fn map_ws_error(operation: &str, error: WsError) -> AppError {
    match error {
        WsError::ConnectionClosed | WsError::AlreadyClosed => transport_closed(error.to_string()),
        WsError::Io(io) if is_retry(io.kind()) => transport_timeout(operation),
        WsError::Io(io) if io.kind() == ErrorKind::ConnectionReset => {
            transport_closed(io.to_string())
        }
        other => transport_failed(operation, other.to_string()),
    }
}

/// Every endpoint is loopback, so a TCP connect that does not complete means
/// nothing accepted it. Windows retries a refused loopback SYN for about two
/// seconds before it reports the refusal, so a deadline shorter than that ends
/// as a timeout there; both outcomes are the same unavailable endpoint.
fn connect_failed(url: &WebSocketUrl, error: &std::io::Error) -> AppError {
    crate::discovery::connect_error::endpoint_unreachable(url.port(), &error.to_string())
}

#[cfg(test)]
#[path = "websocket_tests.rs"]
mod tests;
