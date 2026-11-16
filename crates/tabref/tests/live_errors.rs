//! Live regressions for how failures and messages reach the agent: an
//! error is one complete line, printed once, from an in-process run, from
//! the daemon and from a failing `do` step; and a message the page shows
//! twice (a summary list and inline under the field) after a click is
//! printed once and whole, not cut mid-word.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_errors`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::Command;

use common::{Harness, Run, harness, live, serve};

const FORM: &str = r#"<!doctype html><title>Book</title>
<form onsubmit="event.preventDefault(); check()">
<ul id=summary></ul>
<label>Time <input id=time value="09:30"></label><p id=inline></p>
<button>Book</button>
</form>
<script>
function check() {
  const times = [];
  for (let h = 8; h < 17; h++) times.push(h + ':15', h + ':45');
  const message = 'Dr. Mora is already booked at 9:30 AM on Dec 29, 2026. Free times that day: ' + times.join(', ') + '.';
  document.getElementById('summary').innerHTML = '<li>Please fix the following:</li><li></li>';
  document.querySelector('#summary li:last-child').textContent = message;
  document.getElementById('inline').textContent = message;
}
</script>"#;

const MISSING: &str = "Archive every closed ticket older than ninety days in the support queue now";

fn run(tabref: &Harness, args: &[&str], daemon: bool) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "30000",
        ])
        .args(args)
        .env("TABREF_HOME", &tabref.home)
        .env("TABREF_REFS", "long");
    if daemon {
        command.env_remove("TABREF_NO_DAEMON");
    } else {
        command.env("TABREF_NO_DAEMON", "1");
    }
    let output = command.output().unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

struct Site(Harness);

impl Drop for Site {
    fn drop(&mut self) {
        let _ = run(&self.0, &["daemon", "stop"], true);
    }
}

fn site(name: &str) -> Site {
    let port = serve(&[("/book", FORM)]);
    let tabref = harness(name);
    let opened = run(
        &tabref,
        &["open", &format!("http://127.0.0.1:{port}/book")],
        true,
    );
    assert!(opened.ok, "{}\n{}", opened.stdout, opened.stderr);
    Site(tabref)
}

#[test]
fn an_error_is_printed_once_and_whole_from_every_path() {
    if !live() {
        return;
    }
    let site = site("errors-once");
    let target = format!("button \"{MISSING}\"");
    let batch = format!("fill Time 10:15\nclick {target}\nfill Time 11:15");
    let cases: [(Vec<&str>, bool); 4] = [
        (vec!["click", &target], false),
        (vec!["click", &target], true),
        (vec!["do", &batch], true),
        (vec!["do", &batch], false),
    ];
    for (args, daemon) in cases {
        let failed = run(&site.0, &args, daemon);
        assert!(!failed.ok, "{args:?}");
        let all = format!("{}\n{}", failed.stdout, failed.stderr);
        let errors: Vec<&str> = all
            .lines()
            .filter(|line| line.starts_with("error "))
            .collect();
        assert_eq!(errors.len(), 1, "{args:?} daemon={daemon}:\n{all}");
        assert!(
            errors[0].starts_with("error ELEMENT_NOT_FOUND"),
            "{}",
            errors[0]
        );
        assert!(errors[0].contains(MISSING), "{}", errors[0]);
        assert_eq!(
            all.matches(MISSING).count(),
            if args[0] == "do" { 2 } else { 1 },
            "{args:?} daemon={daemon}:\n{all}"
        );
    }
}

#[test]
fn a_message_the_page_shows_twice_is_printed_once_and_whole() {
    if !live() {
        return;
    }
    let site = site("errors-message");
    let clicked = run(&site.0, &["click", "Book"], true);
    assert!(clicked.ok, "{}\n{}", clicked.stdout, clicked.stderr);
    let lines: Vec<&str> = clicked
        .stdout
        .lines()
        .filter(|line| line.contains("already booked"))
        .collect();
    assert_eq!(lines.len(), 1, "{}", clicked.stdout);
    assert!(
        lines[0].ends_with("16:15, 16:45."),
        "cut: {}\n{}",
        lines[0],
        clicked.stdout
    );
}
