#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use common::{harness, live, scratch};

const MB: usize = 1024 * 1024;
const FIRST: usize = 32 * 1024;

fn enabled() -> bool {
    live() && (cfg!(windows) || std::env::var_os("TABREF_NO_DAEMON").is_some())
}

fn serve(bytes: usize, partial: bool) -> (u16, mpsc::Sender<()>, mpsc::Sender<()>) {
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
                let mut request = [0; 4096];
                let count = stream.read(&mut request).unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..count]);
                if request.starts_with("GET /slow ") {
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
                    let page = "<!doctype html><title>Downloads</title><a href='/slow'>Export</a><button>Continue</button>";
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
                && entry
                    .metadata()
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

fn follow_up(action: bool, partial: bool, bytes: usize) {
    let (port, release, finish) = serve(bytes, partial);
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
    let next = if action {
        tabref.run(&["click", "Continue"])
    } else {
        tabref.run(&["wait", "--download"])
    };
    let text = format!("{}{}", next.stdout, next.stderr);
    if bytes > MB {
        assert!(text.contains("over the 1 MB download cap"), "{text}");
        assert!(text.contains("canceled"), "{text}");
        assert!(!text.contains("downloaded \"slow.bin\""), "{text}");
        assert_eq!(next.ok, action, "{text}");
        assert!(!folder.join("slow.bin").exists());
        assert!(
            !std::fs::read_dir(&folder)
                .unwrap()
                .flatten()
                .any(|entry| entry
                    .metadata()
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
fn wait_removes_a_download_that_finished_over_cap_after_the_click_exited() {
    if enabled() {
        follow_up(false, false, 2 * MB);
    }
}

#[test]
fn an_action_reports_and_removes_a_leftover_completed_download_over_cap() {
    if enabled() {
        follow_up(true, false, 2 * MB);
    }
}

#[test]
fn an_action_cancels_a_leftover_partial_download_over_cap() {
    if enabled() {
        follow_up(true, true, 2 * MB);
    }
}

#[test]
fn wait_cancels_a_leftover_partial_download_over_cap() {
    if enabled() {
        follow_up(false, true, 2 * MB);
    }
}

#[test]
fn wait_still_names_a_download_that_finishes_within_the_cap() {
    if enabled() {
        follow_up(false, false, MB / 2);
    }
}
