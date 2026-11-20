//! Live regressions for downloads through the daemon: a relative download
//! folder means the folder where the command was typed, a download that
//! starts after its command ends is still named while the daemon idles, a
//! popup that downloads and closes is not a failed click, and a download
//! over the size cap is canceled. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_downloads_daemon -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use common::exports::{REPORT, serve_exports};
use common::{Harness, Run, harness, live, scratch};

fn site(name: &str) -> (Harness, String) {
    let port = serve_exports();
    (harness(name), format!("http://127.0.0.1:{port}"))
}

/// `tabref ARGS` typed in `dir`, through the daemon.
fn run_in(tabref: &Harness, dir: &Path, args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "30000",
        ])
        .args(args)
        .current_dir(dir)
        .env("TABREF_HOME", &tabref.home)
        .env("TABREF_REFS", "long")
        .env_remove("TABREF_NO_DAEMON")
        .output()
        .unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Makes tabref treat the harness browser as one `open --launch` started.
fn adopt_browser(tabref: &Harness) {
    let record = serde_json::to_string(&tabref.browser).unwrap();
    std::fs::write(tabref.home.join("launched.json"), record).unwrap();
    common::private_record(&tabref.home.join("launched.json"));
}

#[test]
fn a_relative_download_folder_is_the_one_where_the_command_was_typed() {
    if !live() {
        return;
    }
    let (tabref, base) = site("downloads-relative");
    let outside = scratch("downloads-relative");
    let work = outside.0.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let opened = run_in(
        &tabref,
        &work,
        &["open", "--download-dir", "dl", &format!("{base}/")],
    );
    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    let clicked = run_in(&tabref, &work, &["click", "Export"]);
    assert!(clicked.ok, "{}{}", clicked.stdout, clicked.stderr);
    let saved = work.join("dl").join("report.csv");
    assert_eq!(
        std::fs::read_to_string(&saved).unwrap(),
        REPORT,
        "{}",
        clicked.stdout
    );
}

#[test]
fn a_download_that_starts_after_its_command_is_named_while_the_daemon_idles() {
    if !live() {
        return;
    }
    let (tabref, base) = site("downloads-late");
    adopt_browser(&tabref);
    let outside = scratch("downloads-late");
    let folder = outside.0.join("late");
    let dir = folder.to_str().unwrap();
    tabref.ok(&["open", "--download-dir", dir, &format!("{base}/")]);
    tabref.ok(&["click", "Later export"]);
    std::thread::sleep(Duration::from_millis(3_500));
    let names: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["report.csv".to_owned()], "{names:?}");
    let waited = tabref.ok(&["wait", "--download"]);
    assert!(
        waited.stdout.contains("downloaded \"report.csv\""),
        "{}",
        waited.stdout
    );
}

#[test]
fn a_popup_that_downloads_and_closes_is_a_download_not_a_lost_tab() {
    if !live() {
        return;
    }
    let (tabref, base) = site("downloads-popup");
    let outside = scratch("downloads-popup");
    let folder = outside.0.join("popup");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        &format!("{base}/"),
    ]);
    let clicked = tabref.ok(&["click", "Popup export"]);
    assert!(
        clicked.stdout.contains("downloaded \"report.csv\""),
        "{}",
        clicked.stdout
    );
    assert!(folder.join("report.csv").is_file());
}

#[test]
fn a_download_over_the_size_cap_is_canceled() {
    if !live() {
        return;
    }
    let (tabref, base) = site("downloads-cap");
    let outside = scratch("downloads-capped");
    let folder = outside.0.join("capped");
    tabref.ok(&[
        "open",
        "--download-dir",
        folder.to_str().unwrap(),
        "--download-max-mb",
        "1",
        &format!("{base}/"),
    ]);
    let clicked = tabref.ok(&["click", "Big export"]);
    assert!(
        clicked.stdout.contains("over the 1 MB download cap"),
        "{}",
        clicked.stdout
    );
    assert!(!folder.join("big.bin").exists());
}

#[test]
fn the_state_directory_is_refused_as_a_download_folder() {
    if !live() {
        return;
    }
    let (tabref, base) = site("downloads-state");
    let home = tabref.home.to_str().unwrap().to_owned();
    let inside = tabref.home.join("downloads").join("mine");
    for dir in [home.as_str(), inside.to_str().unwrap()] {
        let run = tabref.run(&["open", "--download-dir", dir, &format!("{base}/")]);
        assert!(
            run.stderr.starts_with("error INVALID_ARGS: ")
                && run.stderr.contains("state directory"),
            "{dir}: {}{}",
            run.stdout,
            run.stderr
        );
    }
    assert!(!inside.exists());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_missing_profile_is_refused_through_its_resolved_path() {
    if !live() {
        return;
    }
    let (tabref, base) = site("downloads-profile-alias");
    let outside = scratch("downloads-profile-alias");
    let real_var = outside.0.canonicalize().unwrap().join("private/var");
    std::fs::create_dir_all(&real_var).unwrap();
    let var = outside.0.join("var");
    std::os::unix::fs::symlink(&real_var, &var).unwrap();
    #[cfg(target_os = "linux")]
    let (env, relative) = ("XDG_CONFIG_HOME", "google-chrome");
    #[cfg(target_os = "macos")]
    let (env, relative) = ("HOME", "Library/Application Support/Google/Chrome");
    let profile = real_var.join("home").join(relative).join("Default");
    let port = tabref.browser.port.to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--cdp", &port, "--timeout", "30000"])
        .args([
            "open",
            "--download-dir",
            profile.to_str().unwrap(),
            &format!("{base}/"),
        ])
        .env("TABREF_HOME", &tabref.home)
        .env(env, var.join("home"))
        .env("TABREF_NO_DAEMON", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let saved = tabref.home.join(format!("download-dir-{port}"));
    assert!(
        !output.status.success()
            && stderr.starts_with("error INVALID_ARGS: ")
            && stderr.contains("profile directory")
            && !real_var.join("home").exists()
            && !saved.exists(),
        "created profile: {}, saved setting: {}, stderr: {stderr}",
        profile.exists(),
        saved.exists(),
    );
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn a_browsers_own_profile_directory_is_refused_as_a_download_folder() {
    if !live() {
        return;
    }
    let (tabref, base) = site("downloads-profile");
    let outside = scratch("downloads-profile");
    let root = outside.0.join("browser-home");
    #[cfg(target_os = "linux")]
    let (env, relative) = ("XDG_CONFIG_HOME", "google-chrome");
    #[cfg(target_os = "macos")]
    let (env, relative) = ("HOME", "Library/Application Support/Google/Chrome");
    #[cfg(target_os = "windows")]
    let (env, relative) = ("LOCALAPPDATA", "Google/Chrome/User Data");
    let profile = root.join(relative).join("Default");
    let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "30000",
        ])
        .args([
            "open",
            "--download-dir",
            profile.to_str().unwrap(),
            &format!("{base}/"),
        ])
        .env("TABREF_HOME", &tabref.home)
        .env(env, &root)
        .env("TABREF_NO_DAEMON", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("error INVALID_ARGS: ") && stderr.contains("profile directory"),
        "{stderr}"
    );
    assert!(!output.status.success());
    assert!(!root.exists());
}
