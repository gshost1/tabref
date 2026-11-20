#![cfg(windows)]

use std::fs::{self, OpenOptions};
use std::net::TcpListener;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_cdp::{LaunchedBrowser, ProfileUse};
use tabref_core::RefStore;
use tungstenite::Message;

#[test]
fn time_windows_reconnect() {
    let temp = std::env::var_os("RUNNER_TEMP").unwrap();
    let home = Path::new(&temp).join("tabref-reconnect-fixture");
    let profile = home.join("profile");
    fs::create_dir_all(&profile).unwrap();
    let _lock = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(profile.join("lockfile"))
        .unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let websocket_path = "/devtools/browser/timing";
    fs::write(
        profile.join("DevToolsActivePort"),
        format!("{port}\n{websocket_path}\n"),
    )
    .unwrap();
    let record = LaunchedBrowser {
        pid: std::process::id(),
        port,
        websocket_path: websocket_path.to_owned(),
        profile_dir: profile,
        headless: true,
        started: None,
        profile_use: ProfileUse::THROWAWAY,
    };
    RefStore::new(&home)
        .write_state("launched.json", &serde_json::to_string(&record).unwrap())
        .unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut socket = tungstenite::accept(stream.unwrap()).unwrap();
            while let Ok(Message::Text(message)) = socket.read() {
                let request: Value = serde_json::from_str(&message).unwrap();
                assert_eq!(request["method"], "Target.getTargets");
                let response = json!({"id": request["id"], "result": {"targetInfos": []}});
                socket
                    .send(Message::Text(response.to_string().into()))
                    .unwrap();
            }
        }
    });

    let before = Path::new(&temp).join("before.exe");
    let after = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/tabref.exe");
    let mut samples = [Vec::new(), Vec::new()];
    for step in 0..17 {
        let order = if step % 2 == 0 { [0, 1] } else { [1, 0] };
        for index in order {
            let binary = [&before, &after][index];
            let start = Instant::now();
            let output = Command::new(binary)
                .args(["--json", "--timeout", "5000", "tabs"])
                .env("TABREF_HOME", &home)
                .env("TABREF_NO_DAEMON", "1")
                .env_remove("TABREF_CDP")
                .output()
                .unwrap();
            let elapsed = start.elapsed();
            let result: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert!(output.status.success(), "{binary:?}: {result}");
            assert_eq!(result["data"]["tabs"], json!([]), "{result}");
            if step >= 2 {
                samples[index].push(elapsed);
            }
        }
    }
    for (label, values) in ["before", "after"].into_iter().zip(&mut samples) {
        values.sort_unstable();
        let millis = |d: Duration| d.as_secs_f64() * 1000.0;
        println!(
            "{label}: n={} min={:.2}ms median={:.2}ms max={:.2}ms",
            values.len(),
            millis(values[0]),
            millis(values[values.len() / 2]),
            millis(*values.last().unwrap())
        );
    }
}
