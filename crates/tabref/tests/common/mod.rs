#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod exports;
pub mod focus_page;
pub mod lazy_site;
#[cfg(unix)]
pub mod own_chrome;
#[cfg(unix)]
pub mod slow_serve;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

static HOMES: AtomicU32 = AtomicU32::new(0);

use serde_json::Value;
use tabref_cdp::{LaunchOptions, LaunchedBrowser, launch_chrome};
use tabref_core::Deadline;

pub fn live() -> bool {
    let on = std::env::var_os("TABREF_LIVE").is_some_and(|value| value == "1");
    if !on {
        eprintln!("skipped: set TABREF_LIVE=1 to run live Chrome tests");
    }
    on
}

/// A headless Chrome and a private state directory for the CLI, both
/// removed when dropped.
pub struct Harness {
    pub browser: LaunchedBrowser,
    pub home: PathBuf,
}

impl Drop for Harness {
    fn drop(&mut self) {
        kill_group(self.browser.pid);
        let _ = std::fs::remove_dir_all(&self.browser.profile_dir);
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

#[cfg(unix)]
fn kill_group(pid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .status();
}

#[cfg(windows)]
fn kill_group(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

/// The extra Chrome arguments `TABREF_CHROME_ARGS` names (CI passes
/// `--no-sandbox`), split on whitespace.
pub fn chrome_args() -> Vec<String> {
    std::env::var("TABREF_CHROME_ARGS")
        .map(|args| args.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

pub fn harness(name: &str) -> Harness {
    let options = LaunchOptions {
        extra_args: chrome_args(),
        ..LaunchOptions::default()
    };
    let browser = launch_chrome(&options, Deadline::after(30_000).unwrap()).unwrap();
    Harness {
        browser,
        home: harness_home(name),
    }
}

/// A directory outside any state directory, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn scratch(name: &str) -> Scratch {
    Scratch(harness_home(&format!("scratch-{name}")))
}

/// Makes a launch record a test wrote private to the user, as tabref writes
/// it: tabref ignores a record that others could write.
#[cfg(unix)]
pub fn private_record(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
pub fn private_record(_path: &std::path::Path) {}

/// A fresh private state directory for one test.
pub fn harness_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "tabref-cli-{name}-{}-{}",
        std::process::id(),
        HOMES.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    home
}

/// What one `tabref` run printed and whether it succeeded.
pub struct Run {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    pub fn json(&self) -> Value {
        serde_json::from_str(self.stdout.trim())
            .unwrap_or_else(|error| panic!("not JSON ({error}): {}\n{}", self.stdout, self.stderr))
    }

    /// The ref nearest before the first `needle` on its line: the ref of
    /// the control token (`[@s1:e4 button Save]`) that holds `needle`.
    pub fn ref_for(&self, needle: &str) -> String {
        let at = self
            .stdout
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle} in:\n{}", self.stdout));
        let line_start = self.stdout[..at].rfind('\n').map_or(0, |end| end + 1);
        let start = self.stdout[line_start..at]
            .rfind("@s")
            .map(|offset| line_start + offset)
            .unwrap_or_else(|| panic!("no ref before {needle} in:\n{}", self.stdout));
        self.stdout[start..]
            .split(|c: char| c.is_whitespace() || c == ']')
            .next()
            .unwrap_or_default()
            .to_owned()
    }
}

impl Harness {
    pub fn run(&self, args: &[&str]) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
        command.args(["--cdp", &self.browser.port.to_string()]);
        if !args.contains(&"--timeout") {
            command.args(["--timeout", "30000"]);
        }
        let output = command
            .args(args)
            .env("TABREF_HOME", &self.home)
            .env("TABREF_REFS", "long")
            .output()
            .unwrap();
        Run {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    pub fn ok(&self, args: &[&str]) -> Run {
        let run = self.run(args);
        assert!(
            run.ok,
            "tabref {args:?} failed:\n{}\n{}",
            run.stdout, run.stderr
        );
        run
    }
}

/// Serves each `(path, html)` on a loopback port and returns the port.
pub fn serve(pages: &[(&str, &str)]) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let pages: Arc<HashMap<String, String>> = Arc::new(
        pages
            .iter()
            .map(|(path, html)| ((*path).to_owned(), (*html).to_owned()))
            .collect(),
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let pages = Arc::clone(&pages);
            std::thread::spawn(move || answer(stream, &pages));
        }
    });
    port
}

fn answer(mut stream: TcpStream, pages: &HashMap<String, String>) {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => request.extend_from_slice(&chunk[..read]),
        }
    }
    let head = String::from_utf8_lossy(&request);
    let path = head
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_owned();
    let (status, body) = match pages.get(&path) {
        Some(html) => ("200 OK", html.clone()),
        None => (
            "404 Not Found",
            "<!doctype html><title>404</title>".to_owned(),
        ),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}
