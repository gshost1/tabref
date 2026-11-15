//! A fake browser for tests that need no real one: it writes its port to the
//! profile's `DevToolsActivePort`, holds the port, and never answers. Used
//! as `TABREF_CHROME` (through `silent_browser`) or started on its own in a
//! process group of its own and recorded as tabref's launch would
//! (`start_detached`, `launch_record`). Linux only. Included with `#[path]`.
#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SILENT_BROWSER: &str = "import os, socket, sys, time
profile = next(a.split('=', 1)[1] for a in sys.argv[1:] if a.startswith('--user-data-dir='))
s = socket.socket()
s.bind(('127.0.0.1', 0))
s.listen()
open(os.environ['FAKE_PID_FILE'], 'w').write(str(os.getpid()))
open(os.path.join(profile, 'DevToolsActivePort'), 'w').write('%d\\n/devtools/browser/x\\n' % s.getsockname()[1])
held = []
while True:
    held.append(s.accept()[0])
";

/// A `TABREF_CHROME` binary in `dir` that runs the silent browser.
pub fn silent_browser(dir: &Path) -> PathBuf {
    let script = script(dir);
    let binary = dir.join("silent-browser");
    std::fs::write(
        &binary,
        format!("#!/bin/sh\nexec python3 {} \"$@\"\n", script.display()),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    binary
}

/// Starts the silent browser on `profile`, leading a process group of its
/// own and not a child of this test (so once killed it is reaped, not left a
/// zombie), and returns its pid and port.
pub fn start_detached(tools: &Path, profile: &Path) -> (u32, u16) {
    std::fs::create_dir_all(profile).unwrap();
    let pid_file = tools.join("detached-pid");
    let _ = std::fs::remove_file(&pid_file);
    let line = format!(
        "setsid python3 '{}' '--user-data-dir={}' </dev/null >/dev/null 2>&1 &",
        script(tools).display(),
        profile.display()
    );
    std::process::Command::new("sh")
        .args(["-c", &line])
        .env("FAKE_PID_FILE", &pid_file)
        .status()
        .unwrap();
    let started = Instant::now();
    loop {
        let pid = std::fs::read_to_string(&pid_file).unwrap_or_default();
        let port = std::fs::read_to_string(profile.join("DevToolsActivePort")).unwrap_or_default();
        if let (Ok(pid), Some(Ok(port))) = (pid.trim().parse(), port.lines().next().map(str::parse))
        {
            return (pid, port);
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the silent browser did not start"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// When process `pid` started, as tabref records it (`/proc/<pid>/stat`).
pub fn start_time(pid: u32) -> String {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let after_name = &stat[stat.rfind(')').unwrap() + 1..];
    after_name.split_whitespace().nth(19).unwrap().to_owned()
}

/// The launch record tabref keeps for a browser it started.
pub fn launch_record(pid: u32, port: u16, profile: &Path, persistent: bool) -> String {
    serde_json::json!({
        "pid": pid,
        "port": port,
        "websocket_path": "/devtools/browser/x",
        "profile_dir": profile,
        "headless": true,
        "started": start_time(pid),
        "persistent": persistent,
    })
    .to_string()
}

/// Whether process `pid` is gone, waiting up to five seconds for it.
pub fn gone(pid: u32) -> bool {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let started = Instant::now();
    while proc.exists() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    !proc.exists()
}

pub fn kill(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}

fn script(dir: &Path) -> PathBuf {
    let script = dir.join("silent.py");
    std::fs::write(&script, SILENT_BROWSER).unwrap();
    script
}
