//! `tabref close --browser` quits the browser `open --launch` started and
//! nothing else: with no launch record, or with `--cdp`, it refuses; a
//! record whose pid is now another program, even with that program's real
//! start time, never ends it nor deletes the folder it names; an adopted
//! browser is only asked to close; a browser of tabref's own that ignores
//! `Browser.close` is killed with its process group; a throwaway profile
//! goes and a `--profile` one stays. These use the silent fake browser, so
//! no real one is needed.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/profile_home.rs"]
mod profile_home;
#[path = "common/silent_browser.rs"]
mod silent;

use std::path::Path;

use common::{Run, harness_home};
use profile_home::{Dirs, launch_on, tabref, tabref_command};
use silent::{gone, kill, launch_record, start_detached, start_time, write_record};

/// `close --browser` with `temp` as the temp folder.
fn close_browser(home: &Path, temp: &Path) -> Run {
    let output = tabref_command(
        home,
        &home.with_extension("config"),
        &["--json", "--timeout", "3000", "close", "--browser"],
        true,
    )
    .env("TMPDIR", temp)
    .output()
    .unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A `sleep` leading a process group of its own, as `setsid` starts one,
/// not a child of this test.
fn detached_sleep(tools: &Path) -> u32 {
    let pid_file = tools.join("sleep-pid");
    std::process::Command::new("sh")
        .args([
            "-c",
            &format!(
                "setsid sh -c 'echo $$ > {}; exec sleep 300' </dev/null >/dev/null 2>&1 &",
                pid_file.display()
            ),
        ])
        .status()
        .unwrap();
    let started = std::time::Instant::now();
    loop {
        let text = std::fs::read_to_string(&pid_file).unwrap_or_default();
        if let Ok(pid) = text.trim().parse::<u32>() {
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            if comm.trim() == "sleep" {
                return pid;
            }
        }
        assert!(started.elapsed().as_secs() < 10, "the sleep did not start");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn with_no_browser_tabref_launched_it_refuses() {
    let home = harness_home("close-none");
    let dirs = Dirs {
        homes: vec![home.clone()],
    };
    let run = close_browser(&home, &home);
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
fn a_record_naming_another_program_by_its_real_start_time_never_ends_it_nor_deletes_its_folder() {
    let home = harness_home("close-forged");
    let tools = harness_home("close-forged-tools");
    let temp = harness_home("close-forged-temp");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), temp.clone()],
    };
    let pid = detached_sleep(&tools);
    let mine = tools.join("tabref-profile-mine");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("keep"), "x").unwrap();
    write_record(&home, &launch_record(pid, 9, &mine, false, false));
    assert!(!start_time(pid).is_empty());

    let run = close_browser(&home, &temp);
    std::thread::sleep(std::time::Duration::from_millis(200));
    let alive = std::path::Path::new(&format!("/proc/{pid}")).exists();
    kill(pid);
    let kept = mine.join("keep").exists();
    drop(dirs);

    assert!(
        alive,
        "close --browser killed a program tabref did not launch"
    );
    assert!(
        kept,
        "close --browser deleted a folder outside the temp folder"
    );
    assert!(!run.ok, "{}", run.stdout);
}

#[test]
fn an_adopted_browser_is_asked_to_close_and_never_killed() {
    let home = harness_home("close-adopted");
    let tools = harness_home("close-adopted-tools");
    let profiles = harness_home("close-adopted-dir");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), profiles.clone()],
    };
    let profile = profiles.join("work");
    let (pid, port) = start_detached(&tools, &profile);
    write_record(&home, &launch_record(pid, port, &profile, true, true));

    let run = close_browser(&home, &profiles);
    let alive = std::path::Path::new(&format!("/proc/{pid}")).exists();
    kill(pid);
    drop(dirs);

    assert!(alive, "an adopted browser was killed");
    assert!(!run.ok, "{}", run.stdout);
    assert_eq!(
        run.json()["error"]["details"]["kind"],
        "browser_not_closed",
        "{}",
        run.stdout
    );
}

#[test]
fn a_mismatched_port_file_blocks_close_and_attachment_without_killing_the_browser() {
    let home = harness_home("close-mismatch");
    let tools = harness_home("close-mismatch-tools");
    let profiles = harness_home("close-mismatch-profiles");
    let _dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), profiles.clone()],
    };
    let profile = profiles.join("work");
    let (pid, port) = start_detached(&tools, &profile);
    let record = launch_record(pid, port, &profile, true, false);
    write_record(&home, &record);
    std::fs::write(
        profile.join("DevToolsActivePort"),
        format!("{port}\n/devtools/browser/another-run\n"),
    )
    .unwrap();

    let config = home.with_extension("config");
    for result in [
        close_browser(&home, &profiles),
        tabref(&home, &config, &["--json", "tabs"], true),
        launch_on(&home, &config, &profile),
    ] {
        assert!(!result.ok, "{}", result.stdout);
        let error = &result.json()["error"];
        assert_eq!(error["details"]["kind"], "cdp_launch_unverified", "{error}");
        let hint = error["suggestion"].as_str().unwrap();
        assert!(hint.contains(&format!("pid {pid}")), "{hint}");
        assert!(hint.contains(profile.to_str().unwrap()), "{hint}");
        assert!(
            hint.contains("tabref open <url> --launch --profile <profile-dir>"),
            "{hint}"
        );
    }
    assert_eq!(
        start_time(pid),
        serde_json::from_str::<serde_json::Value>(&record).unwrap()["started"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(home.join("launched.json")).unwrap(),
        record
    );
}

#[test]
fn a_browser_that_ignores_browser_close_is_killed_and_its_throwaway_profile_removed() {
    let home = harness_home("close-silent");
    let tools = harness_home("close-silent-tools");
    let temp = harness_home("close-silent-temp");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), temp.clone()],
    };
    let profile = std::fs::canonicalize(&temp)
        .unwrap()
        .join("tabref-profile-s0silent");
    let (pid, port) = start_detached(&tools, &profile);
    write_record(&home, &launch_record(pid, port, &profile, false, false));

    let run = close_browser(&home, &temp);
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
    let temp = harness_home("close-kept-temp");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), temp.clone()],
    };
    let profile = std::fs::canonicalize(&temp)
        .unwrap()
        .join("tabref-profile-s00named");
    let (pid, port) = start_detached(&tools, &profile);
    write_record(&home, &launch_record(pid, port, &profile, true, false));

    let run = close_browser(&home, &temp);
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

#[test]
fn a_recorded_pid_that_another_program_now_has_is_never_ended() {
    let home = harness_home("close-reused");
    let tools = harness_home("close-reused-tools");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone()],
    };
    let pid = detached_sleep(&tools);
    let profile = tools.join("tabref-profile-s0reused");
    let mut record: serde_json::Value =
        serde_json::from_str(&launch_record(pid, 9, &profile, false, false)).unwrap();
    record["started"] = "1".into();
    write_record(&home, &record.to_string());

    let run = close_browser(&home, &tools);
    let alive = std::path::Path::new(&format!("/proc/{pid}")).exists();
    kill(pid);
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
