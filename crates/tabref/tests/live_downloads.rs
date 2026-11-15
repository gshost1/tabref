//! Live runs of downloads: a folder chosen with `open --download-dir`, a
//! click that downloads reporting the saved file, a slow download that the
//! click leaves running and `wait --download` returns, an `open` of an
//! address that downloads, and a second download of the same name kept
//! beside the first. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_downloads -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use common::{Harness, harness, live};

const PAGE: &str = r#"<!doctype html><title>Exports</title><h1>Exports</h1>
<a href="/report.csv">Export</a>
<a href="/slow.bin">Slow export</a>
<a href="/notes">Notes</a>"#;
const NOTES: &str = "<!doctype html><title>Notes</title><p>Just a page</p>";
const REPORT: &str = "name,price\nHoodie,62.00\n";
const SLOW_BYTES: usize = 3_000;

/// Serves the export page, a CSV sent as an attachment, and a binary
/// attachment whose first third arrives at once and the rest two seconds
/// later.
fn serve_exports() -> u16 {
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

fn site() -> (Harness, String, PathBuf) {
    let port = serve_exports();
    let tabref = harness("downloads");
    let folder = tabref.home.join("saved");
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        &format!("{base}/"),
    ]);
    (tabref, base, folder)
}

#[test]
fn a_click_that_downloads_reports_the_saved_file_and_wait_returns_it() {
    if !live() {
        return;
    }
    let (tabref, base, folder) = site();
    let clicked = tabref.ok(&["click", "Export"]);
    let saved = folder.join("report.csv");
    assert!(
        clicked.stdout.contains(&format!(
            "downloaded \"report.csv\" ({} bytes) -> {}",
            REPORT.len(),
            saved.display()
        )),
        "{}",
        clicked.stdout
    );
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), REPORT);
    let waited = tabref.ok(&["--json", "wait", "--download"]).json();
    let download = &waited["data"]["download"];
    assert_eq!(download["name"], "report.csv", "{waited}");
    assert_eq!(download["path"], saved.to_str().unwrap(), "{waited}");
    assert_eq!(download["url"], format!("{base}/report.csv"), "{waited}");
    assert_eq!(download["bytes"], REPORT.len(), "{waited}");
    assert!(download["elapsed_ms"].is_u64(), "{waited}");
    let again = tabref.ok(&["click", "Export"]);
    assert!(again.stdout.contains("report (1).csv"), "{}", again.stdout);
}

#[test]
fn a_slow_download_the_click_leaves_running_is_returned_by_wait() {
    if !live() {
        return;
    }
    let (tabref, _, folder) = site();
    let clicked = tabref.ok(&["--timeout", "1500", "click", "Slow export"]);
    assert!(
        clicked
            .stdout
            .contains("download \"slow.bin\" still running; tabref wait --download"),
        "{}",
        clicked.stdout
    );
    let waited = tabref.ok(&["wait", "--download"]);
    assert!(
        waited
            .stdout
            .contains(&format!("downloaded \"slow.bin\" ({SLOW_BYTES} bytes)")),
        "{}",
        waited.stdout
    );
    assert_eq!(
        std::fs::read(folder.join("slow.bin")).unwrap().len(),
        SLOW_BYTES
    );
}

#[test]
fn opening_an_address_that_downloads_reports_the_file_and_keeps_the_page() {
    if !live() {
        return;
    }
    let (tabref, base, folder) = site();
    let opened = tabref.ok(&["open", &format!("{base}/report.csv")]);
    assert!(
        opened.stdout.starts_with("downloaded \"report.csv\""),
        "{}",
        opened.stdout
    );
    assert!(folder.join("report.csv").is_file());
    let page = tabref.ok(&["js", "document.title"]);
    assert_eq!(page.stdout.trim(), "Exports");
}

#[test]
fn wait_for_a_download_when_none_comes_times_out_honestly() {
    if !live() {
        return;
    }
    let (tabref, _, _) = site();
    tabref.ok(&["click", "Notes"]);
    let waited = tabref.run(&["--timeout", "800", "wait", "--download"]);
    assert!(
        waited.stderr.starts_with("error TIMEOUT"),
        "{}{}",
        waited.stdout,
        waited.stderr
    );
    assert!(waited.stderr.contains("download"), "{}", waited.stderr);
}

#[test]
fn a_batch_opens_a_download_and_expects_one() {
    if !live() {
        return;
    }
    let (tabref, _, folder) = site();
    let run = tabref.ok(&["do", "open /report.csv; click \"Export\"; expect download"]);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(
        lines[0].contains("downloaded \"report.csv\""),
        "{}",
        run.stdout
    );
    assert!(
        lines[1].contains("downloaded \"report (1).csv\""),
        "{}",
        run.stdout
    );
    assert!(lines[2].starts_with("ok expect download"), "{}", run.stdout);
    assert!(folder.join("report (1).csv").is_file());
}
