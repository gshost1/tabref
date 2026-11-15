//! Live regressions for an app that embeds Chrome for its own interface
//! (Electron), reached over `--cdp`: tabref reads and acts on the app's
//! window, and `open` refuses to navigate it away or open a new window,
//! while a normal Chrome on a file page still opens a web page. No Electron
//! app is installed here, so the fixture is headless Chrome with an
//! Electron user agent whose window shows a web page, as Slack's does.
//! Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_app_guard -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{Harness, chrome_args, harness, harness_home, live, serve};
use serde_json::Value;
use tabref_cdp::{LaunchOptions, launch_chrome};
use tabref_core::Deadline;

const ELECTRON_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Notes/1.4.2 Chrome/128.0.0.0 Electron/32.1.0 Safari/537.36";

const APP_PAGE: &str = r#"<!doctype html><title>Notes app</title><h1>My notes</h1>
<p id=out>No note yet</p>
<button onclick="document.getElementById('out').textContent = 'New note made'">New note</button>"#;

const WEB_PAGE: &str = "<!doctype html><title>Help</title><h1>Help center</h1>";

/// A browser that says it is an Electron app.
fn app_harness(name: &str) -> Harness {
    let mut extra_args = chrome_args();
    extra_args.push(format!("--user-agent={ELECTRON_AGENT}"));
    let options = LaunchOptions {
        extra_args,
        ..LaunchOptions::default()
    };
    let browser = launch_chrome(&options, Deadline::after(30_000).unwrap()).unwrap();
    Harness {
        browser,
        home: harness_home(name),
    }
}

/// The app's interface as a local file, the way an Electron app ships it.
fn app_file(tabref: &Harness) -> PathBuf {
    let file = tabref.home.join("index.html");
    std::fs::write(&file, APP_PAGE).unwrap();
    file
}

#[test]
fn an_apps_window_is_read_and_acted_on_but_never_opened_over_or_beside() {
    if !live() {
        return;
    }
    let tabref = app_harness("app-guard");
    let port = serve(&[("/app", APP_PAGE), ("/help", WEB_PAGE)]);
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&["js", &format!("location.href = '{base}/app'; 1")]);
    tabref.ok(&["wait", "--text", "My notes"]);
    let shown = tabref.ok(&["snap"]);
    assert!(shown.stdout.contains("My notes"), "{}", shown.stdout);
    let clicked = tabref.ok(&["click", "New note"]);
    assert!(
        clicked.stdout.contains("New note made"),
        "{}",
        clicked.stdout
    );

    let refused = tabref.run(&["--json", "open", &format!("{base}/help")]);
    let error = &refused.json()["error"];
    assert_eq!(error["code"], "ACTION_NOT_SUPPORTED", "{error}");
    assert_eq!(error["details"]["kind"], "cdp_not_a_browser", "{error}");
    assert_eq!(error["disposition"]["delivery"], "not_delivered", "{error}");
    assert_eq!(error["details"]["runtime"], "Electron/32.1.0", "{error}");
    assert_eq!(
        error["suggestion"],
        "Read and act on the app's page with snap, click and fill (js to read it); open web addresses in a web browser",
        "{error}"
    );
    let beside = tabref.run(&["open", "--new-tab", &format!("{base}/help")]);
    assert!(
        beside
            .stderr
            .starts_with("error ACTION_NOT_SUPPORTED (cdp_not_a_browser)"),
        "{}",
        beside.stderr
    );
    let step = tabref.run(&["do", "open /help"]);
    assert!(
        step.stderr
            .starts_with("error ACTION_NOT_SUPPORTED (cdp_not_a_browser)"),
        "{}",
        step.stderr
    );
    let tabs = tabref.ok(&["tabs"]);
    assert_eq!(tabs.stdout.lines().count(), 1, "{}", tabs.stdout);
    let still = tabref.ok(&["js", "document.title"]);
    assert_eq!(still.stdout.trim(), "Notes app");
}

#[test]
fn a_file_page_in_a_normal_chrome_still_opens_a_web_page() {
    if !live() {
        return;
    }
    let port = serve(&[("/help", WEB_PAGE)]);
    let help = format!("http://127.0.0.1:{port}/help");
    let chrome = harness("app-guard-chrome");
    let file = app_file(&chrome);
    chrome.ok(&["open", &format!("file://{}", file.display())]);
    let moved = chrome.ok(&["open", &help]);
    assert!(moved.stdout.contains("Help center"), "{}", moved.stdout);
}

#[test]
fn a_batch_with_an_open_in_an_app_is_refused_before_any_step_runs() {
    if !live() {
        return;
    }
    let tabref = app_harness("app-guard-batch");
    let port = serve(&[("/app", APP_PAGE), ("/help", WEB_PAGE)]);
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&["js", &format!("location.href = '{base}/app'; 1")]);
    tabref.ok(&["wait", "--text", "My notes"]);
    let run = tabref.run(&["--json", "do", "click \"New note\"; open /help"]);
    let error = &run.json()["error"];
    assert_eq!(error["details"]["kind"], "cdp_not_a_browser", "{error}");
    assert_eq!(error["disposition"]["delivery"], "not_delivered", "{error}");
    let out = tabref.ok(&["js", "document.getElementById('out').textContent"]);
    assert_eq!(out.stdout.trim(), "No note yet");
}

#[test]
fn a_batch_that_fails_after_a_step_ran_is_not_safe_to_retry() {
    if !live() {
        return;
    }
    let chrome = harness("app-guard-retry");
    let file = app_file(&chrome);
    chrome.ok(&["open", &format!("file://{}", file.display())]);
    let run = chrome.run(&["--json", "do", "click \"New note\"; click \"Nope\""]);
    let error = &run.json()["error"];
    assert_eq!(error["code"], "ELEMENT_NOT_FOUND", "{error}");
    assert_eq!(error["disposition"]["retry"], "unsafe", "{error}");
}

/// Asks the browser's own HTTP endpoint for `path`, as a user closing a
/// window from outside tabref would, and returns the body of its `200`
/// answer.
fn devtools_get(tabref: &Harness, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", tabref.browser.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer: Vec<u8> = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).unwrap_or(0);
        if read == 0 {
            break;
        }
        answer.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&answer).into_owned();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(usize::MAX);
            if body.len() >= length {
                break;
            }
        }
    }
    let text = String::from_utf8_lossy(&answer).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    assert!(head.starts_with("HTTP/1.1 200"), "{path}: {text}");
    body.to_owned()
}

/// The page targets `/json/list` names.
fn pages(tabref: &Harness) -> Vec<Value> {
    let listed: Value = serde_json::from_str(&devtools_get(tabref, "/json/list")).unwrap();
    listed
        .as_array()
        .unwrap()
        .iter()
        .filter(|target| target["type"] == "page")
        .cloned()
        .collect()
}

/// Closes every page from outside tabref and waits, up to 5 s, until the
/// browser lists none.
fn close_every_page(tabref: &Harness) {
    for page in pages(tabref) {
        let id = page["id"].as_str().unwrap().to_owned();
        let answer = devtools_get(tabref, &format!("/json/close/{id}"));
        assert!(answer.contains("Target is closing"), "{answer}");
    }
    let started = Instant::now();
    while !pages(tabref).is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            pages(tabref)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn close_is_refused_in_an_app_and_its_window_stays() {
    if !live() {
        return;
    }
    let tabref = app_harness("app-guard-close");
    let port = serve(&[("/app", APP_PAGE)]);
    tabref.ok(&[
        "js",
        &format!("location.href = 'http://127.0.0.1:{port}/app'; 1"),
    ]);
    tabref.ok(&["wait", "--text", "My notes"]);
    let refused = tabref.run(&["--json", "close"]);
    let error = &refused.json()["error"];
    assert_eq!(error["code"], "ACTION_NOT_SUPPORTED", "{error}");
    assert_eq!(error["details"]["kind"], "cdp_not_a_browser", "{error}");
    assert_eq!(error["details"]["runtime"], "Electron/32.1.0", "{error}");
    let named = tabref.run(&["close", "0"]);
    assert!(
        named.stderr.starts_with("error ACTION_NOT_SUPPORTED"),
        "{}",
        named.stderr
    );
    let tabs = tabref.ok(&["tabs"]);
    assert_eq!(tabs.stdout.lines().count(), 1, "{}", tabs.stdout);
}

#[test]
fn an_app_with_no_window_page_gets_no_blank_tab_and_lists_no_tabs() {
    if !live() {
        return;
    }
    let tabref = app_harness("app-guard-empty");
    close_every_page(&tabref);
    let snap = tabref.run(&["--json", "snap"]);
    let error = &snap.json()["error"];
    assert_eq!(error["details"]["kind"], "cdp_app_has_no_page", "{error}");
    let tabs = tabref.ok(&["--json", "tabs"]).json();
    assert_eq!(tabs["data"]["tabs"], serde_json::json!([]), "{tabs}");
    assert!(pages(&tabref).is_empty(), "{:?}", pages(&tabref));
}

#[test]
fn a_normal_chrome_with_no_page_gets_a_blank_tab() {
    if !live() {
        return;
    }
    let chrome = harness("app-guard-empty-chrome");
    close_every_page(&chrome);
    chrome.ok(&["snap"]);
    let left = pages(&chrome);
    assert_eq!(left.len(), 1, "{left:?}");
    assert_eq!(left[0]["url"], "about:blank", "{left:?}");
}
