#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

pub fn harness(name: &str) -> Harness {
    let extra_args = std::env::var("TABREF_CHROME_ARGS")
        .map(|args| args.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
    let options = LaunchOptions {
        extra_args,
        ..LaunchOptions::default()
    };
    let browser = launch_chrome(&options, Deadline::after(30_000).unwrap()).unwrap();
    let home = std::env::temp_dir().join(format!(
        "tabref-cli-{name}-{}-{}",
        std::process::id(),
        HOMES.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    Harness { browser, home }
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

    /// The first ref on the line that contains `needle`.
    pub fn ref_for(&self, needle: &str) -> String {
        let line = self
            .stdout
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line with {needle}:\n{}", self.stdout));
        line.split_whitespace()
            .find(|word| word.starts_with("@s"))
            .unwrap_or_else(|| panic!("no ref on: {line}"))
            .to_owned()
    }
}

impl Harness {
    pub fn run(&self, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
            .args(["--cdp", &self.browser.port.to_string()])
            .args(args)
            .env("TABREF_HOME", &self.home)
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
