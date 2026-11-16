//! The state and profile directories of the `open --launch --profile`
//! tests, and a `tabref` that runs with them. Included with `#[path]`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::common::Run;

/// `tabref` with `home` as its state directory and `config` as the user's
/// config and home folder (so a launched Chrome never writes in the real
/// home, and the "user's own profiles" are under `config`).
pub fn tabref(home: &Path, config: &Path, args: &[&str], in_process: bool) -> Run {
    tabref_command(home, config, args, in_process)
        .output()
        .map_or_else(
            |error| panic!("{error}"),
            |output| Run {
                ok: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            },
        )
}

pub fn tabref_command(home: &Path, config: &Path, args: &[&str], in_process: bool) -> Command {
    let _ = std::fs::create_dir_all(config);
    let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
    command
        .args(args)
        .env("TABREF_HOME", home)
        .env("HOME", config)
        .env("XDG_CONFIG_HOME", config)
        .env_remove("TABREF_CDP")
        .env_remove("TABREF_NO_DAEMON");
    if in_process {
        command.env("TABREF_NO_DAEMON", "1");
    }
    command
}

pub fn launched_pid(home: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(home.join("launched.json")).ok()?;
    let record: serde_json::Value = serde_json::from_str(&text).ok()?;
    u32::try_from(record["pid"].as_u64()?).ok()
}

/// Ends the browser `pid` the way a user closing it does (SIGTERM, so it
/// writes its profile out), and waits until it is gone.
pub fn stop_browser(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status();
    let started = Instant::now();
    while Path::new(&format!("/proc/{pid}")).exists() && started.elapsed() < Duration::from_secs(10)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .stderr(std::process::Stdio::null())
        .status();
}

/// Kills every browser whose command line names `dir`, so no test leaves
/// one running (a leaked one also keeps the build slot it inherited).
pub fn kill_browsers_in(dir: &Path) {
    let needle = dir.to_string_lossy().into_owned();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let cmdline = std::fs::read(entry.path().join("cmdline")).unwrap_or_default();
        if pid != std::process::id() && String::from_utf8_lossy(&cmdline).contains(&needle) {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .status();
        }
    }
}

/// The test's state and profile directories, cleaned up (browsers on them
/// killed first) however the test ends.
pub struct Dirs {
    pub homes: Vec<PathBuf>,
}

impl Drop for Dirs {
    fn drop(&mut self) {
        for home in &self.homes {
            let config = home.with_extension("config");
            let _ = tabref(home, &config, &["daemon", "stop"], false);
            kill_browsers_in(home);
            let _ = std::fs::remove_dir_all(home);
        }
    }
}

/// `open --launch --profile DIR` in-process, as JSON.
pub fn launch_on(home: &Path, config: &Path, dir: &Path) -> Run {
    let dir = dir.to_string_lossy().into_owned();
    tabref(
        home,
        config,
        &[
            "--json",
            "open",
            "about:blank",
            "--launch",
            "--profile",
            &dir,
        ],
        true,
    )
}
