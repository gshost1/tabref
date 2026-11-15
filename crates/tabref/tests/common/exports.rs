//! A loopback server for the download tests: an export page, a CSV sent
//! as an attachment, a slow binary attachment, a 2 MB one, and links that
//! download in a popup or from a timer.

#![allow(dead_code, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

const PAGE: &str = r#"<!doctype html><title>Exports</title><h1>Exports</h1>
<a href="/report.csv">Export</a>
<a href="/slow.bin">Slow export</a>
<a href="/notes">Notes</a>
<a href="/report.csv" target="_blank">Popup export</a>
<a href="/big.bin">Big export</a>
<button onclick="setTimeout(() => { location.href = '/report.csv'; }, 1500)">Later export</button>"#;
const NOTES: &str = "<!doctype html><title>Notes</title><p>Just a page</p>";
pub const REPORT: &str = "name,price\nHoodie,62.00\n";
pub const SLOW_BYTES: usize = 3_000;
pub const BIG_BYTES: usize = 2 * 1024 * 1024;

/// Serves the export page, a CSV sent as an attachment, and a binary
/// attachment whose first third arrives at once and the rest two seconds
/// later.
pub fn serve_exports() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            thread::spawn(move || answer(stream));
        }
    });
    port
}

fn answer(mut stream: TcpStream) {
    let mut request = [0_u8; 4096];
    let read = stream.read(&mut request).unwrap_or(0);
    let head = String::from_utf8_lossy(&request[..read]).into_owned();
    let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
    let (kind, disposition, body, delay) = match path.as_str() {
        "/report.csv" => (
            "text/csv",
            Some("report.csv"),
            REPORT.as_bytes().to_vec(),
            0,
        ),
        "/slow.bin" => (
            "application/octet-stream",
            Some("slow.bin"),
            vec![7_u8; SLOW_BYTES],
            2_000,
        ),
        "/big.bin" => (
            "application/octet-stream",
            Some("big.bin"),
            vec![1_u8; BIG_BYTES],
            0,
        ),
        "/notes" => ("text/html", None, NOTES.as_bytes().to_vec(), 0),
        _ => ("text/html", None, PAGE.as_bytes().to_vec(), 0),
    };
    let disposition = disposition
        .map(|name| format!("Content-Disposition: attachment; filename=\"{name}\"\r\n"))
        .unwrap_or_default();
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\n{disposition}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(headers.as_bytes());
    let first = if delay > 0 {
        body.len() / 3
    } else {
        body.len()
    };
    let _ = stream.write_all(&body[..first]);
    let _ = stream.flush();
    thread::sleep(Duration::from_millis(delay));
    let _ = stream.write_all(&body[first..]);
}
