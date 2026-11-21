#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/held_download.rs"]
mod held_download;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use common::{harness, live, scratch};

const MB: usize = 1024 * 1024;
const FIRST: usize = 32 * 1024;

fn enabled() -> bool {
    if !live() {
        return false;
    }
    if !cfg!(windows) && std::env::var_os("TABREF_NO_DAEMON").is_none() {
        eprintln!(
            "skipped: set TABREF_NO_DAEMON=1 to exercise downloads between in-process commands"
        );
        return false;
    }
    true
}

fn serve(bytes: usize, partial: bool) -> (u16, mpsc::Sender<()>, mpsc::Sender<()>) {
    serve_path(bytes, partial, false)
}

fn serve_path(
    bytes: usize,
    partial: bool,
    long_url: bool,
) -> (u16, mpsc::Sender<()>, mpsc::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (release, released) = mpsc::channel();
    let (finish, finished) = mpsc::channel();
    let gates = Arc::new(Mutex::new((released, finished)));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let gates = Arc::clone(&gates);
            std::thread::spawn(move || {
                let mut request = Vec::new();
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n")
                    && request.len() < 131_072
                {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).unwrap_or(0);
                    if count == 0 {
                        return;
                    }
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8_lossy(&request);
                if request.starts_with("GET /slow") {
                    let headers = b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=slow.bin\r\nConnection: close\r\n\r\n";
                    if stream.write_all(headers).is_err()
                        || stream.write_all(&vec![1; FIRST]).is_err()
                    {
                        return;
                    }
                    let _ = stream.flush();
                    let gates = gates.lock().unwrap();
                    if gates.0.recv_timeout(Duration::from_secs(30)).is_err() {
                        return;
                    }
                    if stream.write_all(&vec![2; bytes - FIRST]).is_err() {
                        return;
                    }
                    let _ = stream.flush();
                    if partial {
                        let _ = gates.1.recv_timeout(Duration::from_secs(30));
                    }
                } else {
                    let query = if long_url {
                        format!("?{}", "x".repeat(70_000))
                    } else {
                        String::new()
                    };
                    let page = format!(
                        "<!doctype html><title>Downloads</title><a href='/slow{query}'>Export</a><button>Continue</button>"
                    );
                    let reply = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                        page.len()
                    );
                    let _ = stream.write_all(reply.as_bytes());
                }
            });
        }
    });
    (port, release, finish)
}

fn await_file(folder: &Path, bytes: usize, partial: bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let ready = std::fs::read_dir(folder).unwrap().flatten().any(|entry| {
            entry.file_name().to_string_lossy().ends_with(".crdownload") == partial
                && std::fs::metadata(entry.path())
                    .is_ok_and(|metadata| metadata.len() >= bytes as u64)
        });
        if ready {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "download did not reach {bytes} bytes (partial={partial})"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn follow_up(command: &[&str], partial: bool, bytes: usize) {
    follow_up_server(command, partial, bytes, serve(bytes, partial));
}

fn follow_up_server(
    command: &[&str],
    partial: bool,
    bytes: usize,
    server: (u16, mpsc::Sender<()>, mpsc::Sender<()>),
) {
    let (port, release, finish) = server;
    let tabref = harness("downloads-no-daemon");
    let outside = scratch("downloads-no-daemon");
    let folder = outside.0.join("saved");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        "--download-max-mb",
        "1",
        &format!("http://127.0.0.1:{port}/"),
    ]);
    let clicked = tabref.ok(&["click", "Export"]);
    assert!(
        clicked.stdout.contains("still running"),
        "{}",
        clicked.stdout
    );
    release.send(()).unwrap();
    await_file(&folder, bytes, partial);
    let next = tabref.run(command);
    let text = format!("{}{}", next.stdout, next.stderr);
    if bytes > MB {
        assert!(!text.contains("downloaded \"slow.bin\""), "{text}");
        assert_eq!(next.ok, !command.contains(&"wait"), "{text}");
        if command.contains(&"--json") {
            let report = next.json();
            assert_eq!(report["data"]["value"], 1);
            let download = &report["data"]["downloads"][0];
            assert_eq!(download["name"], "slow.bin");
            assert_eq!(download["state"]["too_large"]["cap_mb"], 1);
            assert!(download["path"].is_null());
        } else {
            assert!(text.contains("over the 1 MB download cap"), "{text}");
            assert!(
                text.contains(if partial { "canceled" } else { "removed" }),
                "{text}"
            );
        }
        assert!(!folder.join("slow.bin").exists());
        assert!(
            !std::fs::read_dir(&folder)
                .unwrap()
                .flatten()
                .any(|entry| std::fs::metadata(entry.path())
                    .is_ok_and(|metadata| metadata.len() > MB as u64)),
            "over-cap download left in {folder:?}"
        );
    } else {
        assert!(next.ok, "{text}");
        assert!(text.contains("downloaded \"slow.bin\""), "{text}");
        assert_eq!(
            std::fs::metadata(folder.join("slow.bin")).unwrap().len(),
            bytes as u64
        );
    }
    let _ = finish.send(());
}

#[test]
fn a_seventy_kilobyte_url_cannot_disable_cap_reconciliation() {
    if enabled() {
        follow_up_server(
            &["--json", "js", "1"],
            true,
            2 * MB,
            serve_path(2 * MB, true, true),
        );
    }
}

#[test]
fn a_corrupt_ledger_is_a_hook_note_and_is_never_overwritten() {
    if !enabled() {
        return;
    }
    let (port, _, _) = serve(MB / 2, false);
    let tabref = harness("downloads-corrupt");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let path = tabref
        .home
        .join(format!("downloads-{}", tabref.browser.port));
    let bytes = vec![b' '; 70_000];
    std::fs::write(&path, &bytes).unwrap();
    let next = tabref.ok(&["--json", "js", "1"]).json();
    assert_eq!(next["data"]["value"], 1);
    assert_eq!(
        next["data"]["download_reconciliation_error"]["details"]["kind"],
        "download_ledger_unreadable"
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let next = tabref.ok(&["click", "Continue"]);
    assert!(
        next.stdout
            .contains("note: download reconciliation failed:")
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
#[cfg(unix)]
fn an_unsafe_folder_does_not_prevent_canceling_watched_transfers() {
    if !enabled() {
        return;
    }
    let (port, release, _finish) = serve(2 * MB, true);
    let tabref = harness("downloads-unsafe-cancel");
    let outside = scratch("downloads-unsafe-cancel");
    let folder = outside.0.join("saved");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        "--download-max-mb",
        "1",
        &format!("http://127.0.0.1:{port}/"),
    ]);
    tabref.ok(&["click", "Export"]);
    release.send(()).unwrap();
    await_file(&folder, 2 * MB, true);
    let moved = outside.0.join("original");
    std::fs::rename(&folder, &moved).unwrap();
    std::os::unix::fs::symlink(&tabref.home, &folder).unwrap();
    let next = tabref.ok(&["--json", "js", "1"]).json();
    assert_eq!(next["data"]["value"], 1);
    assert!(next["data"]["download_reconciliation_error"].is_object());
    let ledger = std::fs::read_dir(&tabref.home)
        .unwrap()
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("downloads-")
                && !entry.file_name().to_string_lossy().ends_with(".lock")
        })
        .unwrap();
    let kept: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ledger.path()).unwrap()).unwrap();
    assert_eq!(kept[0]["record"]["state"], "canceled", "{kept}");
}

#[test]
fn wait_removes_a_download_that_finished_over_cap_after_the_click_exited() {
    if enabled() {
        follow_up(&["wait", "--download"], false, 2 * MB);
    }
}

#[test]
fn an_action_reports_and_removes_a_leftover_completed_download_over_cap() {
    if enabled() {
        follow_up(&["click", "Continue"], false, 2 * MB);
    }
}

#[test]
fn an_action_cancels_a_leftover_partial_download_over_cap() {
    if enabled() {
        follow_up(&["click", "Continue"], true, 2 * MB);
    }
}

#[test]
fn wait_cancels_a_leftover_partial_download_over_cap() {
    if enabled() {
        follow_up(&["wait", "--download"], true, 2 * MB);
    }
}

#[test]
fn wait_still_names_a_download_that_finishes_within_the_cap() {
    if enabled() {
        follow_up(&["wait", "--download"], false, MB / 2);
    }
}

#[test]
fn snap_reports_a_leftover_completed_download_over_cap() {
    if enabled() {
        follow_up(&["snap"], false, 2 * MB);
    }
}

#[test]
fn js_reports_and_cancels_a_leftover_partial_download_in_json() {
    if enabled() {
        follow_up(&["--json", "js", "1"], true, 2 * MB);
    }
}

#[test]
fn js_without_leftovers_keeps_its_json_shape() {
    if !enabled() {
        return;
    }
    let (port, _, _) = serve(MB / 2, false);
    let tabref = harness("downloads-no-leftovers");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let report = tabref.ok(&["--json", "js", "1"]).json();
    assert_eq!(report["data"], serde_json::json!({ "value": 1 }));
}

#[test]
fn a_held_over_cap_file_is_retried_by_the_next_command() {
    if !enabled() {
        return;
    }
    let (port, release, _) = serve(2 * MB, false);
    let tabref = harness("downloads-held");
    let outside = scratch("downloads-held");
    let folder = outside.0.join("saved");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        "--download-max-mb",
        "1",
        &format!("http://127.0.0.1:{port}/"),
    ]);
    assert!(
        tabref
            .ok(&["click", "Export"])
            .stdout
            .contains("still running")
    );
    release.send(()).unwrap();
    await_file(&folder, 2 * MB, false);
    let path = std::fs::read_dir(&folder)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let held = held_download::hold(&path);
    let first = tabref.ok(&["--json", "js", "1"]).json();
    assert_eq!(first["data"]["value"], 1);
    let download = &first["data"]["downloads"][0];
    assert_eq!(download["state"]["too_large"]["cap_mb"], 1);
    if cfg!(windows) {
        assert!(path.exists());
        assert_eq!(download["removal_pending"], true);
        assert_eq!(download["path"], path.to_string_lossy().as_ref());
        let pending = tabref.ok(&["snap"]).stdout;
        assert!(pending.contains("delete pending:"), "{pending}");
        assert!(!pending.contains("removed"), "{pending}");
    } else {
        assert!(!path.exists());
        assert_eq!(download["removal_pending"], false);
    }
    drop(held);
    let next = tabref.ok(&["--json", "js", "2"]).json();
    assert_eq!(next["data"]["value"], 2);
    if cfg!(windows) {
        assert_eq!(next["data"]["downloads"][0]["removal_pending"], false);
        assert!(next["data"]["downloads"][0]["path"].is_null());
    }
    assert!(!path.exists());
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
}
