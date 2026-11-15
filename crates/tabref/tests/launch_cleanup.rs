//! A browser tabref starts whose DevTools never answers is not left behind:
//! the failed launch ends its process and removes its throwaway profile. A
//! fake browser (`TABREF_CHROME`) writes its port file, holds the port and
//! never answers, so no real browser is needed.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
#[path = "common/profile_home.rs"]
mod profile_home;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

use common::harness_home;
use profile_home::{Dirs, tabref_command};

const SILENT_BROWSER: &str = "import os, socket, sys, time
profile = next(a.split('=', 1)[1] for a in sys.argv[1:] if a.startswith('--user-data-dir='))
s = socket.socket()
s.bind(('127.0.0.1', 0))
s.listen()
open(os.environ['FAKE_PID_FILE'], 'w').write(str(os.getpid()))
open(os.path.join(profile, 'DevToolsActivePort'), 'w').write('%d\\n/devtools/browser/x\\n' % s.getsockname()[1])
held = []
while True:
    held.append(s.accept()[0])
";

fn silent_browser(dir: &Path) -> PathBuf {
    let script = dir.join("silent.py");
    std::fs::write(&script, SILENT_BROWSER).unwrap();
    let binary = dir.join("silent-browser");
    std::fs::write(
        &binary,
        format!("#!/bin/sh\nexec python3 {} \"$@\"\n", script.display()),
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    binary
}

fn gone(pid: &str) -> bool {
    let started = Instant::now();
    while Path::new(&format!("/proc/{pid}")).exists() && started.elapsed() < Duration::from_secs(5)
    {
        std::thread::sleep(Duration::from_millis(50));
    }
    !Path::new(&format!("/proc/{pid}")).exists()
}

/// Runs `open about:blank --launch` and `extra` on the silent browser, and
/// returns the output and whether its process was ended (killing it if not).
fn launch_silent(home: &Path, tools: &Path, temp: &Path, extra: &[&str]) -> (Output, bool) {
    let pid_file = tools.join("pid");
    let mut args = vec!["--timeout", "2000", "open", "about:blank", "--launch"];
    args.extend_from_slice(extra);
    let output = tabref_command(home, &home.with_extension("config"), &args, true)
        .env("TABREF_CHROME", silent_browser(tools))
        .env("FAKE_PID_FILE", &pid_file)
        .env("TMPDIR", temp)
        .output()
        .unwrap();
    let pid = std::fs::read_to_string(&pid_file).unwrap_or_default();
    assert!(!pid.is_empty(), "the fake browser never started");
    let ended = gone(&pid);
    if !ended {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &pid])
            .status();
    }
    (output, ended)
}

#[test]
fn a_launched_browser_that_never_answers_is_ended_with_its_throwaway_profile() {
    let home = harness_home("cleanup");
    let tools = harness_home("cleanup-tools");
    let temp = harness_home("cleanup-temp");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), temp.clone()],
    };
    let (output, ended) = launch_silent(&home, &tools, &temp, &[]);
    let left: Vec<String> = std::fs::read_dir(&temp)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with("tabref-profile-"))
                .collect()
        })
        .unwrap_or_default();
    let recorded = home.join("launched.json").exists();
    drop(dirs);

    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(ended, "the browser that never answered was left running");
    assert!(left.is_empty(), "its throwaway profile was left: {left:?}");
    assert!(!recorded, "a record of the ended browser was kept");
}

#[test]
fn a_browser_on_a_profile_that_never_answers_is_ended_and_the_profile_kept() {
    let home = harness_home("cleanup-p");
    let tools = harness_home("cleanup-p-tools");
    let temp = harness_home("cleanup-p-temp");
    let profiles = harness_home("cleanup-p-dir");
    let dirs = Dirs {
        homes: vec![home.clone(), tools.clone(), temp.clone(), profiles.clone()],
    };
    let profile = profiles.join("work");
    let (output, ended) = launch_silent(
        &home,
        &tools,
        &temp,
        &["--profile", profile.to_str().unwrap()],
    );
    let kept = profile.join(".tabref-profile").exists();
    drop(dirs);

    assert!(!output.status.success());
    assert!(ended, "the browser on the profile was left running");
    assert!(kept, "the persistent profile was removed");
}
