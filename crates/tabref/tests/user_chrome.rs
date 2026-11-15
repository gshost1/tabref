//! `tabref connect --user-chrome`: attaching to the user's own browser is an
//! explicit opt-in, checked like every other endpoint. The "user's browser"
//! here is a profile under a throwaway `XDG_CONFIG_HOME`, so the test never
//! touches the real one. The impostor test needs no browser; the other is
//! live (`TABREF_LIVE=1`).

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{Run, harness_home, live};
use tabref_cdp::{find_chrome, read_active_port};

/// A throwaway config home whose `google-chrome` folder plays the user's
/// default profile.
struct UserHome {
    config: PathBuf,
    state: PathBuf,
}

impl UserHome {
    fn new(name: &str) -> Self {
        let config = harness_home(&format!("{name}-config"));
        std::fs::create_dir_all(config.join("google-chrome")).unwrap();
        Self {
            config,
            state: harness_home(&format!("{name}-state")),
        }
    }

    fn profile(&self) -> PathBuf {
        self.config.join("google-chrome")
    }

    fn tabref(&self, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_tabref"))
            .args(args)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("TABREF_HOME", &self.state)
            .env_remove("TABREF_CDP")
            .env_remove("TABREF_NO_DAEMON")
            .output()
            .unwrap();
        Run {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

impl Drop for UserHome {
    fn drop(&mut self) {
        let _ = self.tabref(&["daemon", "stop"]);
        let _ = std::fs::remove_dir_all(&self.config);
        let _ = std::fs::remove_dir_all(&self.state);
    }
}

fn impostor() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let contacts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&contacts);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            seen.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });
    (port, contacts)
}

#[test]
fn a_profile_pointing_at_another_program_is_never_contacted() {
    let home = UserHome::new("uc-impostor");
    let (port, contacts) = impostor();
    std::fs::write(
        home.profile().join("DevToolsActivePort"),
        format!("{port}\n/devtools/browser/x\n"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        format!("host-{}", std::process::id()),
        home.profile().join("SingletonLock"),
    )
    .unwrap();

    let without = home.tabref(&["--json", "tabs"]);
    let refused = home.tabref(&["--json", "connect", "--user-chrome"]);
    let after = home.tabref(&["--json", "tabs"]);

    assert!(!without.ok, "{}", without.stdout);
    assert!(!refused.ok, "{}", refused.stdout);
    let error = &refused.json()["error"];
    assert_eq!(
        error["details"]["kind"], "cdp_endpoint_unverified",
        "{error}"
    );
    assert!(
        !home.state.join("user_chrome.json").exists(),
        "a refused choice was kept"
    );
    assert!(!after.ok);
    assert_eq!(
        contacts.load(Ordering::SeqCst),
        0,
        "the impostor was sent a connection"
    );
}

#[test]
fn the_users_browser_is_used_only_between_connect_and_off() {
    if !live() {
        return;
    }
    let home = UserHome::new("uc-live");
    let mut browser = Command::new(find_chrome().unwrap())
        .args([
            format!("--user-data-dir={}", home.profile().display()),
            "--headless=new".to_owned(),
            "--no-first-run".to_owned(),
            "--remote-debugging-port=0".to_owned(),
            "about:blank".to_owned(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .unwrap();
    let up = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(100));
        read_active_port(&home.profile()).is_ok()
    });

    let before = home.tabref(&["--json", "tabs"]);
    let connected = home.tabref(&["connect", "--user-chrome"]);
    let opened = home.tabref(&["open", "data:text/html,<h1>In your Chrome</h1>"]);
    let pages = list_pages(&home.profile());
    let off = home.tabref(&["connect", "--off"]);
    let later = home.tabref(&["--json", "tabs"]);

    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{}", browser.id())])
        .status();
    let _ = browser.wait();
    assert!(up, "the user's Chrome never turned remote debugging on");
    assert!(
        !before.ok,
        "used the user's browser without the opt-in: {}",
        before.stdout
    );
    assert!(connected.ok, "{}{}", connected.stdout, connected.stderr);
    assert!(
        connected.stdout.contains("attached to your Google Chrome")
            && connected.stdout.contains("profile ")
            && connected.stdout.contains(&format!("pid {}", browser.id())),
        "{}",
        connected.stdout
    );
    assert!(opened.ok, "{}{}", opened.stdout, opened.stderr);
    assert!(
        pages.contains("In your Chrome") || pages.contains("data:text/html"),
        "{pages}"
    );
    assert!(
        off.ok && off.stdout.contains("stopped using your Google Chrome"),
        "{}",
        off.stdout
    );
    assert!(
        !later.ok,
        "still used the user's browser after --off: {}",
        later.stdout
    );
}

/// The pages the browser of `profile` has open, as its DevTools lists them.
fn list_pages(profile: &Path) -> String {
    let port = read_active_port(profile).unwrap().port;
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    use std::io::{Read, Write};
    stream
        .write_all(b"GET /json/list HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut body = Vec::new();
    let _ = stream.read_to_end(&mut body);
    String::from_utf8_lossy(&body).into_owned()
}
