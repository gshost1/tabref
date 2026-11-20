use std::io::Read;
use std::net::TcpStream;
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Reads one HTTP response from a DevTools server. Chromium answers and then
/// holds the connection open, ignoring `Connection: close`, so reading to EOF
/// would wait out the timeout on every live endpoint: the response is done
/// once Content-Length bytes of body arrive. The read timeout shrinks with
/// what remains of `timeout`, so a server that trickles bytes costs at most
/// `timeout` in all.
pub(crate) fn read_http_response(stream: &mut TcpStream, timeout: Duration) -> Option<Vec<u8>> {
    let started = Instant::now();
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let elapsed = started.elapsed();
        if elapsed >= timeout || stream.set_read_timeout(Some(timeout - elapsed)).is_err() {
            break;
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(received) => {
                raw.extend_from_slice(&chunk[..received]);
                if response_complete(&raw) || raw.len() > MAX_RESPONSE_BYTES {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    if raw.is_empty() { None } else { Some(raw) }
}

pub(crate) fn response_complete(raw: &[u8]) -> bool {
    let Some(header_end) = raw.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let body_received = raw.len() - (header_end + 4);
    content_length(&raw[..header_end]).is_some_and(|expected| body_received >= expected)
}

fn content_length(headers: &[u8]) -> Option<usize> {
    String::from_utf8_lossy(headers).lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    })
}

/// The trimmed body of a `200` response; any other status, or bytes with no
/// header terminator, is `None`.
pub(crate) fn http_ok_body(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status_ok = head
        .lines()
        .next()
        .is_some_and(|status| status.split_whitespace().nth(1) == Some("200"));
    status_ok.then(|| body.trim().to_owned())
}

/// The status code of an HTTP response, or `None` for bytes that do not
/// start with an HTTP status line.
pub(crate) fn http_status(raw: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(raw);
    let mut parts = text.lines().next()?.split_whitespace();
    parts
        .next()
        .filter(|version| version.starts_with("HTTP/"))?;
    parts.next()?.parse().ok()
}

#[cfg(test)]
#[path = "http_response_tests.rs"]
mod tests;
