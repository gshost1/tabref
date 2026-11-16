//! Live regressions for `select`: every form the view prints for an option
//! (its label, `label (value)`, the value) picks it, from a single command
//! and from a `do` step; duplicate labels in two optgroups are told apart by
//! the printed value; a disabled option is refused by name; and an option
//! that is not there fails with one complete error line that lists the
//! options as the view prints them, from the daemon and in-process alike.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_select`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::Command;

use common::{Harness, Run, harness, live, serve};

const FORM: &str = r#"<!doctype html><title>Visit</title>
<label>Clinician <select id=who onchange="document.getElementById('out').textContent = 'who=' + this.value">
<option value="">Pick one</option>
<option value="k07">Dr. Kofi Mensah</option>
<option value="k08">Dr. Kofi Abara</option>
<option value="k09" disabled>Dr. Lena Ortiz</option>
<optgroup label="East"><option value="east-3">Room 3</option></optgroup>
<optgroup label="West"><option value="west-3">Room 3</option></optgroup>
</select></label>
<label>Ward <select id=ward></select></label>
<p id=out>none</p>
<script>
const ward = document.getElementById('ward');
for (let n = 1; n <= 60; n++) ward.add(new Option('Ward number ' + n, 'w' + n));
</script>"#;

fn run(tabref: &Harness, args: &[&str], daemon: bool) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(["--cdp", &tabref.browser.port.to_string(), "--timeout", "30000"])
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

struct Site {
    tabref: Harness,
    view: String,
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = run(&self.tabref, &["daemon", "stop"], true);
    }
}

fn site(name: &str) -> Site {
    let port = serve(&[("/visit", FORM)]);
    let tabref = harness(name);
    let opened = run(
        &tabref,
        &["open", &format!("http://127.0.0.1:{port}/visit")],
        true,
    );
    assert!(opened.ok, "{}\n{}", opened.stdout, opened.stderr);
    Site {
        tabref,
        view: opened.stdout,
    }
}

fn out(site: &Site) -> String {
    let read = run(
        &site.tabref,
        &["js", "document.getElementById('out').textContent"],
        true,
    );
    read.stdout.trim().trim_matches('"').to_owned()
}

fn error_lines(run: &Run) -> Vec<String> {
    format!("{}\n{}", run.stdout, run.stderr)
        .lines()
        .filter(|line| line.starts_with("error "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_option_as_the_view_prints_it_is_selected() {
    if !live() {
        return;
    }
    let site = site("select-printed");
    assert!(
        site.view.contains("Dr. Kofi Abara (k08)"),
        "the view no longer prints the value: {}",
        site.view
    );
    for (choice, value) in [
        ("Dr. Kofi Abara (k08)", "k08"),
        ("Dr. Kofi Mensah", "k07"),
        ("k08", "k08"),
        ("dr. kofi mensah (K07)", "k07"),
    ] {
        let picked = run(&site.tabref, &["select", "Clinician", choice], true);
        assert!(picked.ok, "{choice}: {}\n{}", picked.stdout, picked.stderr);
        assert_eq!(out(&site), format!("who={value}"), "{choice}");
    }
    let batch = run(
        &site.tabref,
        &["do", "select Clinician \"Dr. Kofi Mensah (k07)\""],
        true,
    );
    assert!(batch.ok, "{}\n{}", batch.stdout, batch.stderr);
    assert_eq!(out(&site), "who=k07");
}

#[test]
fn duplicate_labels_in_optgroups_are_told_apart_by_the_printed_value() {
    if !live() {
        return;
    }
    let site = site("select-twins");
    let twins = run(&site.tabref, &["select", "Clinician", "Room 3"], true);
    assert!(!twins.ok);
    let lines = error_lines(&twins);
    assert_eq!(lines.len(), 1, "{}\n{}", twins.stdout, twins.stderr);
    assert!(lines[0].starts_with("error AMBIGUOUS_TARGET"), "{}", lines[0]);
    assert!(
        lines[0].contains("Room 3 (east-3) | Room 3 (west-3)"),
        "{}",
        lines[0]
    );
    let west = run(&site.tabref, &["select", "Clinician", "Room 3 (west-3)"], true);
    assert!(west.ok, "{}\n{}", west.stdout, west.stderr);
    assert_eq!(out(&site), "who=west-3");
}

#[test]
fn a_disabled_option_is_refused_by_name() {
    if !live() {
        return;
    }
    let site = site("select-disabled");
    let refused = run(&site.tabref, &["select", "Clinician", "Dr. Lena Ortiz"], true);
    assert!(!refused.ok);
    let lines = error_lines(&refused);
    assert_eq!(lines.len(), 1, "{}\n{}", refused.stdout, refused.stderr);
    assert!(
        lines[0].contains("Dr. Lena Ortiz (k09)") && lines[0].contains("disabled"),
        "{}",
        lines[0]
    );
    assert_eq!(out(&site), "none");
}

#[test]
fn a_missing_option_fails_once_with_the_whole_list_from_every_path() {
    if !live() {
        return;
    }
    let site = site("select-missing");
    let cases: [(&[&str], bool); 3] = [
        (&["select", "Ward", "Ward number 99"], true),
        (&["select", "Ward", "Ward number 99"], false),
        (&["do", "select Ward \"Ward number 99\"\nselect Ward w2"], true),
    ];
    for (args, daemon) in cases {
        let failed = run(&site.tabref, args, daemon);
        assert!(!failed.ok, "{args:?}");
        let lines = error_lines(&failed);
        assert_eq!(
            lines.len(),
            1,
            "{args:?} daemon={daemon}:\n{}\n{}",
            failed.stdout,
            failed.stderr
        );
        let line = &lines[0];
        assert!(line.starts_with("error ACTION_FAILED"), "{line}");
        assert!(line.contains("Ward number 1 (w1) | Ward number 2 (w2)"), "{line}");
        assert!(line.contains("Ward number 30 (w30)"), "{line}");
        assert!(line.contains("… and 30 more"), "{line}");
        let all = format!("{}{}", failed.stdout, failed.stderr);
        assert_eq!(
            all.matches("Ward number 30 (w30)").count(),
            1,
            "{args:?} daemon={daemon}:\n{all}"
        );
    }
}
