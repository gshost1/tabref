//! Live regressions for an app that embeds Chrome for its own interface
//! (Electron), reached over `--cdp`: tabref reads and acts on the app's own
//! page, and `open` refuses to navigate it away, while a page on the web in
//! the same app, or a normal Chrome on an app-like file page, still opens.
//! No Electron app is installed here, so the fixture is headless Chrome
//! with an Electron user agent showing a `file://` page as its interface.
//! Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_app_guard -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;

use common::{Harness, harness, harness_home, live, serve};
use tabref_cdp::{LaunchOptions, launch_chrome};
use tabref_core::Deadline;

const ELECTRON_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Notes/1.4.2 Chrome/128.0.0.0 Electron/32.1.0 Safari/537.36";

const APP_PAGE: &str = r#"<!doctype html><title>Notes app</title><h1>My notes</h1>
<p id=out>No note yet</p>
<button onclick="document.getElementById('out').textContent = 'New note made'">New note</button>"#;

const WEB_PAGE: &str = "<!doctype html><title>Help</title><h1>Help center</h1>";

/// A browser that says it is an Electron app.
fn app_harness(name: &str) -> Harness {
    let mut extra_args: Vec<String> = std::env::var("TABREF_CHROME_ARGS")
        .map(|args| args.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();
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
fn an_apps_own_page_is_read_and_acted_on_but_never_navigated_away() {
    if !live() {
        return;
    }
    let tabref = app_harness("app-guard");
    let port = serve(&[("/help", WEB_PAGE)]);
    let file = app_file(&tabref);
    let shown = tabref.ok(&["open", &format!("file://{}", file.display())]);
    assert!(shown.stdout.contains("My notes"), "{}", shown.stdout);
    let clicked = tabref.ok(&["click", "New note"]);
    assert!(
        clicked.stdout.contains("New note made"),
        "{}",
        clicked.stdout
    );

    let refused = tabref.run(&["--json", "open", &format!("http://127.0.0.1:{port}/help")]);
    let error = &refused.json()["error"];
    assert_eq!(error["code"], "ACTION_NOT_SUPPORTED", "{error}");
    assert_eq!(error["details"]["kind"], "cdp_not_a_browser", "{error}");
    assert_eq!(error["disposition"]["delivery"], "not_delivered", "{error}");
    let step = tabref.run(&["do", "open /help"]);
    assert!(
        step.stderr
            .starts_with("error ACTION_NOT_SUPPORTED (cdp_not_a_browser)"),
        "{}",
        step.stderr
    );
    let still = tabref.ok(&["js", "document.title"]);
    assert_eq!(still.stdout.trim(), "Notes app");
}

#[test]
fn a_web_page_in_an_app_or_a_file_page_in_chrome_still_opens() {
    if !live() {
        return;
    }
    let port = serve(&[("/help", WEB_PAGE)]);
    let help = format!("http://127.0.0.1:{port}/help");
    let app = app_harness("app-guard-web");
    app.ok(&["open", &help]);
    let again = app.ok(&["open", &format!("{help}?again=1")]);
    assert!(again.stdout.contains("Help center"), "{}", again.stdout);

    let chrome = harness("app-guard-chrome");
    let file = app_file(&chrome);
    chrome.ok(&["open", &format!("file://{}", file.display())]);
    let moved = chrome.ok(&["open", &help]);
    assert!(moved.stdout.contains("Help center"), "{}", moved.stdout);
}
