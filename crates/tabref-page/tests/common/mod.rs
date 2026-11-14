#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

use tabref_cdp::{
    CdpConnection, LaunchOptions, LaunchedBrowser, PageSession, attach_to_target, connect,
    create_background_tab, launch_chrome, navigate,
};
use tabref_core::{AccessibilityNode, Deadline};
use tabref_page::{ElementToken, WebSnapshot};

/// Whether live tests run: they need a real Chrome, so they are opt-in.
pub fn live() -> bool {
    let on = std::env::var_os("TABREF_LIVE").is_some_and(|value| value == "1");
    if !on {
        eprintln!("skipped: set TABREF_LIVE=1 to run live Chrome tests");
    }
    on
}

/// A headless Chrome with a throwaway profile, killed with its whole
/// process group when dropped.
pub struct Chrome {
    pub browser: LaunchedBrowser,
}

impl Drop for Chrome {
    fn drop(&mut self) {
        kill_group(self.browser.pid);
        let _ = std::fs::remove_dir_all(&self.browser.profile_dir);
    }
}

#[cfg(unix)]
fn kill_group(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .status();
}

#[cfg(windows)]
fn kill_group(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

/// Starts headless Chrome with every site in its own process, so a
/// cross-site iframe is always an out-of-process frame.
pub fn launch() -> Chrome {
    let mut extra_args = vec!["--site-per-process".to_owned()];
    if let Ok(args) = std::env::var("TABREF_CHROME_ARGS") {
        extra_args.extend(args.split_whitespace().map(str::to_owned));
    }
    let options = LaunchOptions {
        extra_args,
        ..LaunchOptions::default()
    };
    Chrome {
        browser: launch_chrome(&options, Deadline::after(30_000).unwrap()).unwrap(),
    }
}

pub fn deadline() -> Deadline {
    Deadline::after(15_000).unwrap()
}

pub fn connect_to(chrome: &Chrome) -> CdpConnection {
    connect(&chrome.browser.endpoint().unwrap(), deadline()).unwrap()
}

pub fn open_tab(connection: &mut CdpConnection, url: &str) -> PageSession {
    let target_id = create_background_tab(connection, "about:blank", deadline()).unwrap();
    let session = attach_to_target(connection, &target_id, deadline()).unwrap();
    navigate(connection, &session, url, deadline()).unwrap();
    session
}

/// Serves each `(path, html)` on a loopback port and returns the port. A
/// path not listed is a 404. Each connection gets its own thread.
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

/// The node of `role` named `name`, with its element token.
pub fn find<'a>(
    snapshot: &'a WebSnapshot,
    role: &str,
    name: &str,
) -> (&'a AccessibilityNode, ElementToken) {
    let (node, origin) = snapshot
        .preorder()
        .into_iter()
        .find(|(node, _)| node.role == role && node.display_name() == Some(name))
        .unwrap_or_else(|| panic!("no {role} named {name}: {:#?}", snapshot.tree));
    (node, origin.unwrap().element_token())
}
