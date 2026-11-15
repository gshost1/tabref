use std::io::{BufRead, BufReader, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// The longest request line a daemon reads.
pub(crate) const MAX_REQUEST: u64 = 1024 * 1024;
/// The longest reply line a client reads: a snapshot of a very large page.
pub(crate) const MAX_REPLY: u64 = 512 * 1024 * 1024;

/// Writes `value` as one JSON line.
pub(crate) fn write_line(stream: &mut impl Write, value: &impl Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()
}

/// Reads one JSON line of at most `limit` bytes. `Ok(None)` when the peer
/// closed, or went past the limit, before sending a whole line.
pub(crate) fn read_line<T: DeserializeOwned>(
    stream: impl Read,
    limit: u64,
) -> std::io::Result<Option<T>> {
    let mut reader = BufReader::new(stream.take(limit));
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') {
        return Ok(None);
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(std::io::Error::other)
}
