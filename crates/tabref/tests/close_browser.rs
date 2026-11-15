//! `tabref close --browser` quits the browser `open --launch` started and
//! nothing else: with no launch record, or with `--cdp`, it refuses; a
//! record whose process is now another program never ends that program; a
//! browser that ignores `Browser.close` is killed with its process group;
//! a throwaway profile goes and a `--profile` one stays. These use the
//! silent fake browser, so no real one is needed.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/profile_home.rs"]
mod profile_home;
#[path = "common/silent_browser.rs"]
mod silent;

use std::os::unix::process::CommandExt;
use std::path::Path;

use common::{Run, harness_home};
use profile_home::{Dirs, tabref};
use silent::{gone, kill, launch_record, start_detached, start_time};

fn close_browser(home: &Path) -> Run {
    tabref(
        home,
        &home.with_extension("config"),
        &["--json", "--timeout", "3000", "close", "--browser"],
        true,
    )
}

#[test]
fn with_no_browser_tabref_launched_it_refuses() {
    let home = harness_home("close-none");
    let dirs = Dirs {
        homes: vec![home.clone()],
    };
    let run = close_browser(&home);
    let cdp = tabref(
        &home,
        &home.with_extension("config"),
        &["--json", "--cdp", "9333", "close", "--browser"],
        true,
    );
    drop(dirs);

    assert!(!run.ok, "{}", run.stdout);
    assert_eq!(
        run.json()["error"]["details"]["kind"],
        "no_launched_browser",
        "{}",
        run.stdout
    );
    assert!(!cdp.ok);
    assert_eq!(
        cdp.json()["error"]["details"]["kind"],
        "close_browser_refused",
        "{}",
        cdp.stdout
    );
}

#[test]
fn a_recorded_pid_that_another_program_now_has_is_never_ended() {
    let home = harness_home("close-reused");
    let temp = harness_home("close-reused-temp");
    let dirs = Dirs {
        homes: vec![home.clone(), temp.clone()],
    };
    let mut other = std::process::Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .unwrap();
    let profile = temp.join("tabref-profile-reused");
    std::fs::create_dir_all(&profile).unwrap();
    let mut record: serde_json::Value =
        serde_json::from_str(&launch_record(other.id(), 9, &profile, false)).unwrap();
    record["started"] = "1".into();
    std::fs::write(home.join("launched.json"), record.to_string()).unwrap();

    let run = close_browser(&home);
    let alive = other.try_wait().unwrap().is_none();
    let _ = other.kill();
    let _ = other.wait();
    let recorded = home.join("launched.json").exists();
    drop(dirs);

    assert!(run.ok, "{}{}", run.stdout, run.stderr);
    assert!(
        alive,
        "close --browser ended a program tabref did not launch"
    );
    assert!(!recorded, "the record of a browser that is gone was kept");
    assert_eq!(run.json()["data"]["was_running"], false, "{}", run.stdout);
}

#[test]
fn a_browser_that_ignores_browser_close_is_killed_and_its_throwaway_profile_removed() {
    let home = harness_home("close-silent");
    let tools = harness_home("close-silent-tools");
    let temp = harness_home("close-silent-temp");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), temp.clone()],
    };
    let profile = temp.join("tabref-profile-silent");
    let (pid, port) = start_detached(&tools, &profile);
    std::fs::write(
        home.join("launched.json"),
        launch_record(pid, port, &profile, false),
    )
    .unwrap();

    let run = close_browser(&home);
    let ended = gone(pid);
    if !ended {
        kill(pid);
    }
    let removed = !profile.exists();
    let recorded = home.join("launched.json").exists();
    drop(dirs);

    assert!(run.ok, "{}{}", run.stdout, run.stderr);
    assert!(ended, "the browser was left running");
    assert!(removed, "the throwaway profile was left");
    assert!(!recorded, "the launch record was kept");
    assert_eq!(run.json()["data"]["was_running"], true, "{}", run.stdout);
}

#[test]
fn a_browser_on_a_profile_is_ended_and_the_profile_kept() {
    let home = harness_home("close-kept");
    let tools = harness_home("close-kept-tools");
    let profiles = harness_home("close-kept-dir");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), profiles.clone()],
    };
    let profile = profiles.join("tabref-profile-named");
    let (pid, port) = start_detached(&tools, &profile);
    assert!(!start_time(pid).is_empty());
    std::fs::write(
        home.join("launched.json"),
        launch_record(pid, port, &profile, true),
    )
    .unwrap();

    let run = close_browser(&home);
    let ended = gone(pid);
    if !ended {
        kill(pid);
    }
    let kept = profile.join("DevToolsActivePort").exists();
    drop(dirs);

    assert!(run.ok, "{}{}", run.stdout, run.stderr);
    assert!(ended, "the browser was left running");
    assert!(kept, "a --profile profile was removed");
}
