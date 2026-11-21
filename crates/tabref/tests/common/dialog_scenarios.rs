use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use serde_json::json;
use tabref_cdp::{attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;

use super::attach_checks::CpuLoad;
use super::common::{Harness, Run, harness, harness_home, live};

fn command(home: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--timeout", "10000"])
        .args(args)
        .env("TABREF_HOME", home)
        .env("TABREF_NO_DAEMON", "1")
        .output()
        .unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn page_command(tabref: &Harness, args: &[&str]) -> (Run, Duration) {
    let port = tabref.browser.port.to_string();
    let mut all = vec!["--cdp", &port];
    all.extend(args);
    let started = Instant::now();
    let result = command(&tabref.home, &all);
    (result, started.elapsed())
}

fn check(name: &str, result: Run, elapsed: Duration, must_succeed: bool) {
    eprintln!(
        "scenario {name}: {elapsed:?}, ok={}: {}{}",
        result.ok,
        result.stdout.trim(),
        result.stderr.trim()
    );
    if must_succeed {
        assert!(result.ok, "{name}: {}{}", result.stdout, result.stderr);
        assert_eq!(result.stdout.trim(), "2");
    } else if !result.ok {
        assert!(
            result
                .stderr
                .contains("ACTION_FAILED (cdp_page_unresponsive)"),
            "{}",
            result.stderr
        );
    }
}

fn busy(ms: u64, background: bool) {
    let tabref = harness("dialog-busy-scenario");
    let port = super::common::serve(&[("/", "<title>Busy</title>")]);
    assert!(
        page_command(&tabref, &["open", &format!("http://127.0.0.1:{port}/")])
            .0
            .ok
    );
    let mut connection = connect(
        &tabref.browser.endpoint().unwrap(),
        Deadline::after(5000).unwrap(),
    )
    .unwrap();
    let target = list_page_targets(&mut connection, Deadline::after(5000).unwrap())
        .unwrap()
        .remove(0)
        .target_id;
    if background {
        assert!(
            page_command(&tabref, &["open", "about:blank", "--new-tab"])
                .0
                .ok
        );
        let other = list_page_targets(&mut connection, Deadline::after(5000).unwrap())
            .unwrap()
            .into_iter()
            .find(|page| page.target_id != target)
            .unwrap();
        connection
            .call(
                "Target.activateTarget",
                json!({"targetId":other.target_id}),
                None,
                Deadline::after(5000).unwrap(),
            )
            .unwrap();
    }
    let page = attach_to_target(&mut connection, &target, Deadline::after(5000).unwrap()).unwrap();
    if background {
        let visibility = connection
            .call(
                "Runtime.evaluate",
                json!({"expression":"document.visibilityState"}),
                Some(&page.session_id),
                Deadline::after(5000).unwrap(),
            )
            .unwrap();
        assert_eq!(visibility["result"]["value"], "hidden");
    }
    let (port, received) = server(0, false);
    let pending = connection.send("Runtime.evaluate", json!({"expression":format!("fetch('http://127.0.0.1:{port}/slow',{{mode:'no-cors'}}); const end=Date.now()+{ms}; while(Date.now()<end) {{}}")}), Some(&page.session_id), Deadline::after(5000).unwrap()).unwrap();
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    let (result, elapsed) = page_command(&tabref, &["--tab", "0", "js", "1+1"]);
    check(
        &format!("busy-{ms}-background-{background}"),
        result,
        elapsed,
        ms <= 1000,
    );
    let completed = connection
        .reply(pending, Deadline::after(5000).unwrap())
        .unwrap();
    assert!(completed.get("exceptionDetails").is_none(), "{completed}");
}

fn server(ms: u64, inline: bool) -> (u16, mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sent, received) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                break;
            };
            let sent = sent.clone();
            std::thread::spawn(move || {
                let mut bytes = [0; 2048];
                let read = stream.read(&mut bytes).unwrap_or(0);
                let request = String::from_utf8_lossy(&bytes[..read]);
                if !request.starts_with("GET /slow ") {
                    return;
                }
                let _ = sent.send(());
                let body = if inline {
                    format!(
                        "<title>Slow</title><script>const end=Date.now()+{ms};while(Date.now()<end){{}}</script>Ready"
                    )
                } else {
                    std::thread::sleep(Duration::from_millis(ms));
                    "<title>Slow</title>Ready".into()
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    (port, received)
}

fn navigating(ms: u64, inline: bool) {
    let tabref = harness("dialog-loading-scenario");
    assert!(page_command(&tabref, &["open", "about:blank"]).0.ok);
    let (port, received) = server(ms, inline);
    super::attach_checks::arm(
        &tabref,
        &format!("setTimeout(() => location.href='http://127.0.0.1:{port}/slow',100)"),
    );
    received.recv_timeout(Duration::from_secs(5)).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    let (result, elapsed) = page_command(&tabref, &["js", "1+1"]);
    check(
        &format!("navigation-{ms}-inline-{inline}"),
        result,
        elapsed,
        ms <= 2000,
    );
}

#[test]
fn short_busy_cli_commands_succeed() {
    if !live() {
        return;
    }
    busy(300, false);
    busy(1000, false);
}

#[test]
#[ignore]
fn measure_supervisor_scenarios() {
    if !live() {
        return;
    }
    for ms in [300, 1000, 3000] {
        busy(ms, false);
    }
    for ms in [1000, 3000] {
        navigating(ms, true);
    }
    navigating(2000, false);
    busy(1000, true);
    busy(3000, true);
}

fn cold_batch() {
    let workers: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                let home = harness_home("cold-dialog-load");
                let started = Instant::now();
                let result = command(
                    &home,
                    &["open", "data:text/html,<title>Cold</title>", "--launch"],
                );
                eprintln!(
                    "cold open under load: {:?}, ok={}: {}{}",
                    started.elapsed(),
                    result.ok,
                    result.stdout,
                    result.stderr
                );
                command(&home, &["close", "--browser"]);
                let _ = std::fs::remove_dir_all(home);
                result.ok
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert!(results.iter().all(|ok| *ok), "{results:?}");
}

#[test]
fn four_parallel_cold_launches_succeed_under_cpu_load() {
    if !live() {
        return;
    }
    let _load = CpuLoad::start();
    cold_batch();
}

#[test]
#[ignore]
fn twelve_parallel_cold_launches_succeed_under_cpu_load() {
    if !live() {
        return;
    }
    let _load = CpuLoad::start();
    for _ in 0..3 {
        cold_batch();
    }
}
