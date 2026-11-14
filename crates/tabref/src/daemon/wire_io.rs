use std::io::{BufRead, BufReader, Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

const MAX_LINE: u64 = 512 * 1024 * 1024;

/// Writes `value` as one JSON line.
pub(crate) fn write_line(stream: &mut impl Write, value: &impl Serialize) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(value).map_err(std::io::Error::other)?;
    line.push(b'\n');
    stream.write_all(&line)?;
    stream.flush()
}

/// Reads one JSON line. `Ok(None)` when the peer closed before sending a
/// whole line.
pub(crate) fn read_line<T: DeserializeOwned>(stream: impl Read) -> std::io::Result<Option<T>> {
    let mut reader = BufReader::new(stream.take(MAX_LINE));
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') {
        return Ok(None);
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(std::io::Error::other)
}
