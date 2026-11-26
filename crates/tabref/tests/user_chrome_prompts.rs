//! The user's own browser asks its user to allow every new DevTools
//! connection (remote debugging turned on at chrome://inspect). tabref must
//! not make the user answer more prompts than needed: the connection the
//! user allowed is kept, a retry waits on the prompt already on screen
//! instead of opening another, and commands that come together share one
//! connection. The browser is a fake that counts its prompts, in front of a
//! real headless Chrome (live: `TABREF_LIVE=1`). Linux only.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/approving_browser.rs"]
mod approving_browser;
mod common;
#[path = "common/user_home.rs"]
#[allow(dead_code)]
mod user_home;

use std::process::{Child, Stdio};

use approving_browser::ApprovingBrowser;
use common::{Run, live};
use user_home::UserHome;

fn setup(name: &str) -> (UserHome, ApprovingBrowser) {
    let home = UserHome::new(name);
    let browser = ApprovingBrowser::start(&home.config.join("tools"), &home.profile());
    (home, browser)
}

fn spawn(home: &UserHome, args: &[&str]) -> Child {
    home.command(args, &[])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn finish(child: Child) -> Run {
    let output = child.wait_with_output().unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// `connect --user-chrome`, with prompt 1 answered while it waits.
fn connect_allowed(home: &UserHome, browser: &ApprovingBrowser) -> Run {
    let connect = spawn(home, &["--timeout", "30000", "connect", "--user-chrome"]);
    assert!(browser.wait_for_prompt(1), "connect showed no prompt");
    browser.allow(1);
    finish(connect)
}

#[test]
fn one_allowed_prompt_serves_connect_and_every_later_command() {
    if !live() {
        return;
    }
    let (home, browser) = setup("prompts-once");

    let connected = connect_allowed(&home, &browser);
    let opened = home.tabref(&["open", "data:text/html,<h1>Kept</h1>"]);
    let tabs = home.tabref(&["tabs"]);
    let snap = home.tabref(&["snap"]);

    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);
    assert!(
        connected.stdout.contains("attached to your"),
        "{}",
        connected.stdout
    );
    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    assert!(tabs.ok && snap.ok, "{}{}", tabs.stdout, snap.stdout);
    assert!(snap.stdout.contains("Kept"), "{}", snap.stdout);
    assert_eq!(browser.prompts(), 1, "the user was asked more than once");
}

#[test]
fn a_retry_waits_on_the_prompt_already_on_screen() {
    if !live() {
        return;
    }
    let (home, browser) = setup("prompts-retry");

    let first = home.tabref(&["--json", "--timeout", "1500", "connect", "--user-chrome"]);
    let kept = home.state.join("user_chrome.json").exists();
    let again = home.tabref(&["--json", "--timeout", "1500", "tabs"]);
    let dropped = browser.dropped();
    browser.allow(1);
    let after = home.tabref(&["tabs"]);

    assert!(!first.ok, "{}", first.stdout);
    let error = &first.json()["error"];
    assert_eq!(error["details"]["kind"], "cdp_approval_pending", "{error}");
    assert_eq!(error["details"]["prompt_kept"], true, "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("Allow remote debugging?"),
        "{error}"
    );
    assert!(
        error["suggestion"]
            .as_str()
            .unwrap()
            .contains("opt-in is kept"),
        "{error}"
    );
    assert!(kept, "the opt-in was dropped while its prompt waited");
    assert!(!again.ok, "{}", again.stdout);
    let error = &again.json()["error"];
    assert_eq!(error["details"]["kind"], "cdp_approval_pending", "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("no new prompt was opened"),
        "{error}"
    );
    assert_eq!(dropped, 0, "the waiting connection was dropped");
    assert!(after.ok, "{}{}", after.stdout, after.stderr);
    assert_eq!(browser.prompts(), 1, "a retry opened another prompt");
}

#[test]
fn a_cancelled_prompt_is_reported_as_declined() {
    if !live() {
        return;
    }
    let (home, browser) = setup("prompts-deny");

    let first = home.tabref(&["--json", "--timeout", "1500", "connect", "--user-chrome"]);
    browser.deny(1);
    let declined = home.tabref(&["--json", "--timeout", "3000", "tabs"]);

    assert!(!first.ok, "{}", first.stdout);
    assert!(!declined.ok, "{}", declined.stdout);
    assert_eq!(
        declined.json()["error"]["details"]["kind"],
        "cdp_connection_rejected",
        "{}",
        declined.stdout
    );
    assert_eq!(browser.prompts(), 1);
}

#[test]
fn commands_that_come_together_share_one_connection() {
    if !live() {
        return;
    }
    let (home, browser) = setup("prompts-together");
    let connected = connect_allowed(&home, &browser);
    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);

    let runs: Vec<Child> = (0..3).map(|_| spawn(&home, &["tabs"])).collect();
    let runs: Vec<Run> = runs.into_iter().map(finish).collect();

    for run in &runs {
        assert!(run.ok, "{}{}", run.stdout, run.stderr);
    }
    assert_eq!(
        browser.prompts(),
        1,
        "parallel commands opened more prompts"
    );
}

#[test]
fn without_the_daemon_the_hint_says_the_prompt_no_longer_counts() {
    if !live() {
        return;
    }
    let (home, browser) = setup("prompts-no-daemon");
    let connected = connect_allowed(&home, &browser);
    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);

    let run = home.tabref_with(
        &["--json", "--timeout", "1500", "tabs"],
        &[("TABREF_NO_DAEMON", "1")],
    );

    assert!(!run.ok, "{}", run.stdout);
    let error = &run.json()["error"];
    assert_eq!(error["details"]["kind"], "cdp_approval_pending", "{error}");
    assert_eq!(error["details"]["prompt_kept"], false, "{error}");
    assert!(
        error["suggestion"]
            .as_str()
            .unwrap()
            .contains("Do not just retry"),
        "{error}"
    );
    assert_eq!(browser.prompts(), 2);
}

#[test]
fn connecting_again_to_the_same_browser_asks_nothing() {
    if !live() {
        return;
    }
    let (home, browser) = setup("prompts-again");
    let connected = connect_allowed(&home, &browser);
    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);

    let again = home.tabref(&["--timeout", "3000", "connect", "--user-chrome"]);
    let tabs = home.tabref(&["tabs"]);

    assert!(again.ok, "{}{}", again.stdout, again.stderr);
    assert!(tabs.ok, "{}{}", tabs.stdout, tabs.stderr);
    assert_eq!(browser.prompts(), 1, "a repeated connect asked again");
}
