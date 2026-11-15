//! Live runs of downloads: a folder chosen with `open --download-dir`, a
//! click that downloads reporting the saved file, a slow download that the
//! click leaves running and `wait --download` returns, an `open` of an
//! address that downloads, and a second download of the same name kept
//! beside the first. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_downloads -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;

use common::exports::{REPORT, SLOW_BYTES, serve_exports};
use common::{Harness, Scratch, harness, live, scratch};

fn site() -> (Harness, String, PathBuf, Scratch) {
    let port = serve_exports();
    let tabref = harness("downloads");
    let outside = scratch("downloads");
    let folder = outside.0.join("saved");
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        &format!("{base}/"),
    ]);
    (tabref, base, folder, outside)
}

#[test]
fn a_click_that_downloads_reports_the_saved_file() {
    if !live() {
        return;
    }
    let (tabref, base, folder, _outside) = site();
    let clicked = tabref.ok(&["click", "Export"]);
    let saved = folder.join("report.csv");
    assert!(
        clicked.stdout.contains(&format!(
            "downloaded \"report.csv\" ({} bytes) -> {}",
            REPORT.len(),
            saved.display()
        )),
        "{}",
        clicked.stdout
    );
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), REPORT);
    let json = tabref.ok(&["--json", "click", "Export"]).json();
    let download = &json["data"]["downloads"][0];
    assert_eq!(download["name"], "report (1).csv", "{json}");
    assert_eq!(download["url"], format!("{base}/report.csv"), "{json}");
    assert_eq!(download["bytes"], REPORT.len(), "{json}");
    let again = tabref.ok(&["click", "Export"]);
    assert!(again.stdout.contains("report (2).csv"), "{}", again.stdout);
}

#[test]
fn a_slow_download_the_click_leaves_running_is_returned_by_wait() {
    if !live() {
        return;
    }
    let (tabref, base, folder, _outside) = site();
    let clicked = tabref.ok(&["--timeout", "1500", "click", "Slow export"]);
    assert!(
        clicked
            .stdout
            .contains("download \"slow.bin\" still running; tabref wait --download"),
        "{}",
        clicked.stdout
    );
    let waited = tabref.ok(&["--json", "wait", "--download"]).json();
    let download = &waited["data"]["download"];
    let saved = folder.join("slow.bin");
    assert_eq!(download["name"], "slow.bin", "{waited}");
    assert_eq!(download["path"], saved.to_str().unwrap(), "{waited}");
    assert_eq!(download["url"], format!("{base}/slow.bin"), "{waited}");
    assert_eq!(download["bytes"], SLOW_BYTES, "{waited}");
    assert!(download["elapsed_ms"].is_u64(), "{waited}");
    assert_eq!(std::fs::read(saved).unwrap().len(), SLOW_BYTES);
}

#[test]
fn opening_an_address_that_downloads_reports_the_file_and_keeps_the_page() {
    if !live() {
        return;
    }
    let (tabref, base, folder, _outside) = site();
    let opened = tabref.ok(&["open", &format!("{base}/report.csv")]);
    assert!(
        opened.stdout.starts_with("downloaded \"report.csv\""),
        "{}",
        opened.stdout
    );
    assert!(folder.join("report.csv").is_file());
    let page = tabref.ok(&["js", "document.title"]);
    assert_eq!(page.stdout.trim(), "Exports");
}

#[test]
fn wait_for_a_download_when_none_comes_times_out_honestly() {
    if !live() {
        return;
    }
    let (tabref, _, _, _outside) = site();
    tabref.ok(&["click", "Notes"]);
    let waited = tabref.run(&["--timeout", "800", "wait", "--download"]);
    assert!(
        waited.stderr.starts_with("error TIMEOUT"),
        "{}{}",
        waited.stdout,
        waited.stderr
    );
    assert!(waited.stderr.contains("download"), "{}", waited.stderr);
}

#[test]
fn a_batch_expects_the_download_its_click_left_running() {
    if !live() {
        return;
    }
    let (tabref, _, folder, _outside) = site();
    let run = tabref.ok(&[
        "do",
        "--step-timeout",
        "1500",
        "open /report.csv; click \"Slow export\"; expect download",
    ]);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(
        lines[0].contains("downloaded \"report.csv\""),
        "{}",
        run.stdout
    );
    assert!(
        lines[1].contains("download \"slow.bin\" still running"),
        "{}",
        run.stdout
    );
    assert!(lines[2].starts_with("ok expect download"), "{}", run.stdout);
    assert!(lines[2].contains("\"report.csv\""), "{}", run.stdout);
    let waited = tabref.ok(&["wait", "--download"]);
    assert!(waited.stdout.contains("\"slow.bin\""), "{}", waited.stdout);
    assert!(folder.join("slow.bin").is_file());
}

#[test]
fn a_download_an_action_already_reported_is_not_returned_by_wait() {
    if !live() {
        return;
    }
    let (tabref, _, _, _outside) = site();
    tabref.ok(&["click", "Export"]);
    let waited = tabref.run(&["--timeout", "800", "wait", "--download"]);
    assert!(
        waited.stderr.starts_with("error TIMEOUT"),
        "{}{}",
        waited.stdout,
        waited.stderr
    );
}
