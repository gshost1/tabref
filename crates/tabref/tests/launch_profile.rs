//! `open --launch --profile DIR`: a profile of tabref's own that keeps its
//! logins between runs, reused while its browser runs, and never the user's
//! own profile or tabref's state directory. The refusals need no browser;
//! the rest is live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/profile_home.rs"]
mod profile_home;

use std::os::unix::fs::PermissionsExt;

use common::{Run, harness_home, live, serve};
use profile_home::{Dirs, launch_on, launched_pid, stop_browser, tabref};

const PAGE: &str = "<!doctype html><title>Keep</title><h1>Keep</h1>";

#[test]
fn spellings_that_dodge_the_refusals_are_refused_too() {
    let home = harness_home("profile-dodge");
    let config = harness_home("profile-dodge-config");
    let foreign = harness_home("profile-dodge-foreign");
    let dirs = Dirs {
        homes: vec![home.clone(), config.clone(), foreign.clone()],
    };
    std::fs::create_dir_all(config.join("google-chrome")).unwrap();
    std::fs::write(foreign.join("Local State"), "{}").unwrap();
    std::fs::set_permissions(&foreign, std::fs::Permissions::from_mode(0o700)).unwrap();
    let parent = home.parent().unwrap();
    let home_name = home.file_name().unwrap();
    let cases = [
        config.join("chromium").join("..").join("google-chrome"),
        parent
            .join("missing")
            .join("nope")
            .join("..")
            .join("..")
            .join(home_name)
            .join("p"),
        config.join("google-chrome").join("Default"),
        config.clone(),
        foreign.clone(),
    ];
    let runs: Vec<Run> = cases
        .iter()
        .map(|dir| launch_on(&home, &config, dir))
        .collect();
    let made = (
        config.join("chromium").exists(),
        parent.join("missing").exists(),
    );
    drop(dirs);

    for (dir, run) in cases.iter().zip(&runs) {
        assert!(!run.ok, "{} was launched on: {}", dir.display(), run.stdout);
        assert_eq!(
            run.json()["error"]["details"]["kind"],
            "launch_profile_refused",
            "{}: {}",
            dir.display(),
            run.stdout
        );
    }
    assert!(
        runs[4].stdout.contains("not a tabref profile"),
        "{}",
        runs[4].stdout
    );
    assert_eq!(
        made,
        (false, false),
        "a refused spelling created a directory"
    );
}

#[test]
fn the_users_profile_and_the_state_directory_are_refused() {
    let home = harness_home("profile-refused");
    let config = harness_home("profile-refused-config");
    let dirs = Dirs {
        homes: vec![home.clone(), config.clone()],
    };
    std::fs::create_dir_all(config.join("google-chrome")).unwrap();
    let theirs = config.join("google-chrome");
    let users = tabref(
        &home,
        &config,
        &[
            "--json",
            "open",
            "about:blank",
            "--launch",
            "--profile",
            theirs.to_str().unwrap(),
        ],
        true,
    );
    let inside = home.join("profile");
    let state = tabref(
        &home,
        &config,
        &[
            "--json",
            "open",
            "about:blank",
            "--launch",
            "--profile",
            inside.to_str().unwrap(),
        ],
        true,
    );
    let created = inside.exists();
    drop(dirs);

    assert!(!users.ok);
    let error = &users.json()["error"];
    assert_eq!(
        error["details"]["kind"], "launch_profile_refused",
        "{error}"
    );
    assert!(
        error["suggestion"]
            .as_str()
            .unwrap()
            .contains("connect --user-chrome"),
        "{error}"
    );
    assert!(!state.ok);
    assert_eq!(
        state.json()["error"]["details"]["kind"],
        "launch_profile_refused"
    );
    assert!(!created, "a refused profile was created");
}

#[test]
fn a_profile_keeps_its_data_and_its_running_browser_is_reused() {
    if !live() {
        return;
    }
    let first = harness_home("profile-a");
    let second = harness_home("profile-b");
    let dirs = Dirs {
        homes: vec![first.clone(), second.clone()],
    };
    let config = first.with_extension("config");
    let profile = harness_home("profile-dir").join("work");
    let profiles = Dirs {
        homes: vec![profile.parent().unwrap().to_path_buf()],
    };
    let url = format!("http://127.0.0.1:{}/", serve(&[("/", PAGE)]));
    let launch = [
        "--timeout",
        "30000",
        "open",
        url.as_str(),
        "--launch",
        "--profile",
        profile.to_str().unwrap(),
    ];

    let opened = tabref(&first, &config, &launch, false);
    let mode = std::fs::metadata(&profile).map(|m| m.permissions().mode() & 0o777);
    let record = std::fs::read_to_string(first.join("launched.json")).unwrap_or_default();
    let pid = launched_pid(&first);
    let stored = tabref(
        &first,
        &config,
        &["js", "localStorage.setItem('kept', 'yes'); 1"],
        false,
    );
    let again = tabref(&first, &config, &launch, false);
    let pid_again = launched_pid(&first);
    let elsewhere = tabref(&second, &config, &launch, true);
    let pid_elsewhere = launched_pid(&second);

    let _ = tabref(&first, &config, &["daemon", "stop"], false);
    if let Some(pid) = pid {
        stop_browser(pid);
    }
    let relaunched = tabref(&first, &config, &launch, true);
    let read = tabref(
        &first,
        &config,
        &["js", "localStorage.getItem('kept')"],
        true,
    );
    if let Some(pid) = launched_pid(&first) {
        stop_browser(pid);
    }
    drop(dirs);
    drop(profiles);

    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    assert_eq!(mode.ok(), Some(0o700));
    assert!(record.contains("\"persistent\":true"), "{record}");
    assert!(stored.ok, "{}{}", stored.stdout, stored.stderr);
    assert!(
        again.ok && elsewhere.ok,
        "{}{}{}",
        again.stderr,
        elsewhere.stdout,
        elsewhere.stderr
    );
    assert!(pid.is_some());
    assert_eq!(
        pid_again, pid,
        "a second launch on the profile started another browser"
    );
    assert_eq!(
        pid_elsewhere, pid,
        "a launch from another state directory started another browser"
    );
    assert!(relaunched.ok, "{}{}", relaunched.stdout, relaunched.stderr);
    assert_ne!(launched_pid(&first), pid);
    assert!(
        read.stdout.contains("yes"),
        "the profile lost its data: {}{}",
        read.stdout,
        read.stderr
    );
}

#[test]
fn a_launch_on_a_profile_is_not_served_by_a_kept_connection_to_another_browser() {
    if !live() {
        return;
    }
    let home = harness_home("profile-kept");
    let dirs = Dirs {
        homes: vec![home.clone()],
    };
    let config = home.with_extension("config");
    let profile = harness_home("profile-kept-dir").join("work");
    let profiles = Dirs {
        homes: vec![profile.parent().unwrap().to_path_buf()],
    };
    let throwaway = tabref(
        &home,
        &config,
        &["--timeout", "30000", "open", "about:blank", "--launch"],
        false,
    );
    let first = launched_pid(&home);
    let on_profile = tabref(
        &home,
        &config,
        &[
            "--timeout",
            "30000",
            "open",
            "about:blank",
            "--launch",
            "--profile",
            profile.to_str().unwrap(),
        ],
        false,
    );
    let record = std::fs::read_to_string(home.join("launched.json")).unwrap_or_default();
    let second = launched_pid(&home);
    let _ = tabref(&home, &config, &["daemon", "stop"], false);
    for pid in [first, second].into_iter().flatten() {
        stop_browser(pid);
    }
    drop(dirs);
    drop(profiles);

    assert!(
        throwaway.ok && on_profile.ok,
        "{}{}",
        throwaway.stderr,
        on_profile.stderr
    );
    assert_ne!(
        first, second,
        "the kept connection to the throwaway browser served the profile launch"
    );
    assert!(record.contains(profile.to_str().unwrap()), "{record}");
}
