//! A headless Chrome that stands for the user's own browser in the download
//! tests: started outside tabref, with its own downloads folder set in its
//! profile and its home pointed at a scratch directory, so nothing it saves
//! reaches the real home's Downloads.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tabref_cdp::{LaunchedBrowser, find_chrome, read_active_port};

use super::{Harness, chrome_args, harness_home};

/// A harness whose browser saves its own downloads in `downloads`.
pub fn own_chrome(name: &str, downloads: &Path) -> Harness {
    let profile_dir = harness_home(&format!("{name}-profile"));
    let scratch_home: PathBuf = profile_dir.join("home");
    std::fs::create_dir_all(profile_dir.join("Default")).unwrap();
    std::fs::create_dir_all(&scratch_home).unwrap();
    std::fs::create_dir_all(downloads).unwrap();
    let preferences = serde_json::json!({
        "download": {
            "default_directory": downloads,
            "directory_upgrade": true,
            "prompt_for_download": false,
        },
        "savefile": { "default_directory": downloads },
    });
    std::fs::write(
        profile_dir.join("Default").join("Preferences"),
        preferences.to_string(),
    )
    .unwrap();
    let extra_args = chrome_args();
    let mut child = Command::new(find_chrome().unwrap())
        .args([
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            "--remote-debugging-port=0",
            &format!("--user-data-dir={}", profile_dir.display()),
        ])
        .args(extra_args)
        .arg("about:blank")
        .env("HOME", &scratch_home)
        .env("XDG_CONFIG_HOME", scratch_home.join(".config"))
        .env("XDG_DOWNLOAD_DIR", downloads)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    let started = Instant::now();
    let active = loop {
        if let Ok(active) = read_active_port(&profile_dir) {
            break active;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "Chrome never answered"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    Harness {
        browser: LaunchedBrowser {
            pid,
            port: active.port,
            websocket_path: active.websocket_path,
            profile_dir,
            headless: true,
            started: None,
            profile_use: tabref_cdp::ProfileUse::THROWAWAY,
        },
        home: harness_home(name),
    }
}
