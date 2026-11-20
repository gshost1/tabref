use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use tabref_core::{AppError, Deadline};

use super::connect_error::{endpoint_unreachable, no_http_route};
use super::http_response::{http_ok_body, http_status, read_http_response};
use crate::transport::transport_error::{transport_failed, transport_timeout};

const MIN_IO_SLICE: Duration = Duration::from_millis(1);

pub(crate) fn connect_http(address: SocketAddr, deadline: Deadline) -> Result<TcpStream, AppError> {
    TcpStream::connect_timeout(&address, slice(deadline)?)
        .map_err(|error| endpoint_unreachable(address.port(), &error.to_string()))
}

/// Issues one `GET` to the loopback `address` and returns the body of its `200`
/// response. The reader ends the read once Content-Length
/// bytes of body arrive, because Chromium holds the connection open after
/// answering; every step is bounded by what remains of `deadline`. Any
/// other HTTP status is kind `cdp_no_http_route`: a browser whose remote
/// debugging was turned on at chrome://inspect answers `404` there, as its
/// DevTools server speaks WebSocket only.
pub(crate) fn http_get(
    mut stream: TcpStream,
    address: SocketAddr,
    path: &str,
    deadline: Deadline,
) -> Result<String, AppError> {
    let port = address.port();
    stream
        .set_write_timeout(Some(slice(deadline)?))
        .map_err(|error| transport_failed("http", error.to_string()))?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| transport_failed("http", error.to_string()))?;
    let raw = read_http_response(&mut stream, slice(deadline)?);
    let Some(raw) = raw else {
        if deadline.is_expired() {
            return Err(transport_timeout("http"));
        }
        return Err(transport_failed("http", "the server sent no response"));
    };
    http_ok_body(&raw).ok_or_else(|| match http_status(&raw) {
        Some(status) if status != 200 => no_http_route(port, path, status),
        _ => transport_failed("http", "malformed HTTP response"),
    })
}

fn slice(deadline: Deadline) -> Result<Duration, AppError> {
    if deadline.is_expired() {
        return Err(transport_timeout("http"));
    }
    Ok(deadline.remaining().max(MIN_IO_SLICE))
}

#[cfg(test)]
#[path = "http_get_tests.rs"]
mod tests;
