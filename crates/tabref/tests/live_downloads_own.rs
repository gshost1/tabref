#![cfg(unix)]
//! Live regressions for downloads in a browser tabref did not launch (the
//! user's own): tabref never caps, cancels or reports a download it did not
//! start, a command ending does not take a running wait's folder away, a
//! batch's expect step counts what its own steps reported, and a chosen
//! folder swapped for a link into the state directory is never used. The
//! browser's own downloads go to a scratch folder. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_downloads_own -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use common::exports::{BIG_BYTES, REPORT, serve_exports};
use common::own_chrome::own_chrome;
use common::{Harness, Scratch, live, scratch};

struct Site {
    tabref: Harness,
    base: String,
    own: PathBuf,
    _outside: Scratch,
}

fn site(name: &str) -> Site {
    let port = serve_exports();
    let outside = scratch(name);
    let own = outside.0.join("browser-own");
    let tabref = own_chrome(name, &own);
    Site {
        tabref,
        base: format!("http://127.0.0.1:{port}"),
        own,
        _outside: outside,
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn the_users_own_download_is_never_capped_canceled_or_reported() {
    if !live() {
        return;
    }
    let site = site("own-untouched");
    let tabref = &site.tabref;
    tabref.ok(&["open", "--download-max-mb", "1", &format!("{}/", site.base)]);
    tabref.ok(&["js", "location.href = '/bigslow.bin'; 1"]);
    std::thread::sleep(Duration::from_millis(3_500));
    let saved = site.own.join("bigslow.bin");
    assert_eq!(
        std::fs::metadata(&saved).map(|meta| meta.len()).ok(),
        Some(BIG_BYTES as u64),
        "{:?}",
        names(&site.own)
    );
    let waited = tabref.run(&["--timeout", "800", "wait", "--download"]);
    assert!(
        waited.stderr.starts_with("error TIMEOUT"),
        "{}{}",
        waited.stdout,
        waited.stderr
    );
}

#[test]
fn a_stale_launch_record_on_its_port_does_not_make_the_users_chrome_tabrefs() {
    if !live() {
        return;
    }
    let site = site("own-stale-record");
    let tabref = &site.tabref;
    let mut record = serde_json::to_value(&tabref.browser).unwrap();
    record["pid"] = serde_json::json!(999_999);
    record["started"] = serde_json::Value::Null;
    record["profile_dir"] = serde_json::json!(site._outside.0.join("gone-profile"));
    std::fs::write(tabref.home.join("launched.json"), record.to_string()).unwrap();
    common::private_record(&tabref.home.join("launched.json"));
    tabref.ok(&["open", "--download-max-mb", "1", &format!("{}/", site.base)]);
    tabref.ok(&["js", "location.href = '/bigslow.bin'; 1"]);
    std::thread::sleep(Duration::from_millis(3_500));
    let saved = site.own.join("bigslow.bin");
    assert_eq!(
        std::fs::metadata(&saved).map(|meta| meta.len()).ok(),
        Some(BIG_BYTES as u64),
        "{:?}",
        names(&site.own)
    );
}

#[test]
fn a_command_that_ends_does_not_take_a_running_waits_folder_away() {
    if !live() {
        return;
    }
    let site = site("own-concurrent");
    let tabref = &site.tabref;
    let chosen = site._outside.0.join("chosen");
    tabref.ok(&[
        "open",
        "--download-dir",
        chosen.to_str().unwrap(),
        &format!("{}/", site.base),
    ]);
    let waiting = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "8000",
        ])
        .args(["wait", "--download"])
        .env("TABREF_HOME", &tabref.home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(800));
    tabref.ok(&["js", "1"]);
    tabref.ok(&["js", "location.href = '/report.csv'; 1"]);
    let output = waiting.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        stdout.contains("downloaded \"report.csv\""),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(chosen.join("report.csv")).unwrap(),
        REPORT
    );
    assert!(names(&site.own).is_empty(), "{:?}", names(&site.own));
}

#[test]
fn a_batch_expect_counts_a_download_an_earlier_step_reported() {
    if !live() {
        return;
    }
    let site = site("own-batch");
    let tabref = &site.tabref;
    let chosen = site._outside.0.join("chosen");
    tabref.ok(&[
        "open",
        "--download-dir",
        chosen.to_str().unwrap(),
        &format!("{}/", site.base),
    ]);
    let run = tabref.ok(&["do", "open /report.csv; click \"Export\"; expect download"]);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(lines[2].starts_with("ok expect download"), "{}", run.stdout);
    assert!(lines[2].contains("\"report.csv\""), "{}", run.stdout);
    let later = tabref.run(&["--timeout", "800", "wait", "--download"]);
    assert!(
        later.stderr.starts_with("error TIMEOUT"),
        "{}{}",
        later.stdout,
        later.stderr
    );
}

#[cfg(unix)]
#[test]
fn a_folder_swapped_for_a_link_into_the_state_directory_is_never_used() {
    if !live() {
        return;
    }
    let site = site("own-swapped");
    let tabref = &site.tabref;
    let real = site._outside.0.join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = site._outside.0.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    tabref.ok(&[
        "open",
        "--download-dir",
        link.to_str().unwrap(),
        &format!("{}/", site.base),
    ]);
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&tabref.home, &link).unwrap();
    let clicked = tabref.run(&["click", "Export"]);
    assert!(
        clicked.stderr.contains("no longer safe"),
        "{}",
        clicked.stderr
    );
    assert!(!tabref.home.join("report.csv").exists());
    assert!(
        names(&tabref.home)
            .iter()
            .all(|name| !name.contains("report")),
        "{:?}",
        names(&tabref.home)
    );
    let under_root = link.join("sub");
    let refused = tabref.run(&[
        "open",
        "--download-dir",
        under_root.to_str().unwrap(),
        "about:blank",
    ]);
    assert!(
        refused.stderr.starts_with("error INVALID_ARGS"),
        "{}",
        refused.stderr
    );
    assert!(!tabref.home.join("sub").exists());
    let dangling = site._outside.0.join("dangling");
    std::os::unix::fs::symlink(site._outside.0.join("nowhere"), &dangling).unwrap();
    let broken = tabref.run(&[
        "open",
        "--download-dir",
        dangling.to_str().unwrap(),
        "about:blank",
    ]);
    assert!(
        broken.stderr.starts_with("error INVALID_ARGS") && broken.stderr.contains("does not exist"),
        "{}",
        broken.stderr
    );
}

#[test]
fn a_wait_never_caps_a_download_the_user_started_in_another_tab() {
    if !live() {
        return;
    }
    let site = site("own-other-tab");
    let tabref = &site.tabref;
    let page = format!("{}/", site.base);
    tabref.ok(&["open", "--download-max-mb", "1", &page]);
    tabref.ok(&["open", "--new-tab", &page]);
    tabref.ok(&["tab", "0"]);
    let waiting = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "4000",
        ])
        .args(["wait", "--download"])
        .env("TABREF_HOME", &tabref.home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(800));
    tabref.ok(&["--tab", "1", "js", "location.href = '/bigslow.bin'; 1"]);
    let _ = waiting.wait_with_output().unwrap();
    let saved = site.own.join("bigslow.bin");
    let landed = std::time::Instant::now();
    while std::fs::metadata(&saved).map_or(true, |meta| meta.len() < BIG_BYTES as u64)
        && landed.elapsed() < Duration::from_secs(15)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        std::fs::metadata(site.own.join("bigslow.bin"))
            .map(|meta| meta.len())
            .ok(),
        Some(BIG_BYTES as u64),
        "{:?}",
        names(&site.own)
    );
}

#[cfg(unix)]
#[test]
fn a_chosen_folder_that_became_unsafe_is_an_error_not_a_fallback() {
    if !live() {
        return;
    }
    let site = site("own-unsafe");
    let tabref = &site.tabref;
    let record = serde_json::to_string(&tabref.browser).unwrap();
    std::fs::write(tabref.home.join("launched.json"), record).unwrap();
    common::private_record(&tabref.home.join("launched.json"));
    let real = site._outside.0.join("real");
    std::fs::create_dir_all(&real).unwrap();
    let link = site._outside.0.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    tabref.ok(&[
        "open",
        "--download-dir",
        link.to_str().unwrap(),
        &format!("{}/", site.base),
    ]);
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&tabref.home, &link).unwrap();
    let clicked = tabref.run(&["click", "Export"]);
    assert!(
        clicked
            .stderr
            .contains("is no longer safe; choose another with --download-dir"),
        "{}{}",
        clicked.stdout,
        clicked.stderr
    );
    assert!(names(&site.own).is_empty(), "{:?}", names(&site.own));
    assert!(!tabref.home.join("downloads").join("report.csv").exists());
}
