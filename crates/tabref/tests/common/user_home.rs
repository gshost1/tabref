//! The stand-in for the user's own browser in the `connect --user-chrome`
//! tests: a throwaway config home whose `google-chrome` folder plays the
//! user's default profile, and a headless Chrome on it with remote debugging
//! on. Included with `#[path]` by the tests that need it.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tabref_cdp::{find_chrome, read_active_port};

use crate::common::{Run, harness_home};

pub struct UserHome {
    pub config: PathBuf,
    pub state: PathBuf,
}

impl UserHome {
    pub fn new(name: &str) -> Self {
        let config = harness_home(&format!("{name}-config"));
        std::fs::create_dir_all(config.join("google-chrome")).unwrap();
        Self {
            config,
            state: harness_home(&format!("{name}-state")),
        }
    }

    pub fn profile(&self) -> PathBuf {
        self.config.join("google-chrome")
    }

    /// `tabref` with this home's config and state, and `env` on top.
    pub fn command(&self, args: &[&str], env: &[(&str, &str)]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tabref"));
        command
            .args(args)
            .env("XDG_CONFIG_HOME", &self.config)
            .env("TABREF_HOME", &self.state)
            .env_remove("TABREF_CDP")
            .env_remove("TABREF_NO_DAEMON");
        for (key, value) in env {
            command.env(key, value);
        }
        command
    }

    pub fn tabref(&self, args: &[&str]) -> Run {
        self.tabref_with(args, &[])
    }

    pub fn tabref_with(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let output = self.command(args, env).output().unwrap();
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

/// The "user's" Chrome: headless on the throwaway profile, with remote
/// debugging on as chrome://inspect would turn it on.
pub fn start_user_chrome(home: &UserHome) -> Option<Child> {
    let browser = Command::new(find_chrome().unwrap())
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
            && std::fs::symlink_metadata(home.profile().join("SingletonLock")).is_ok()
    });
    if up {
        return Some(browser);
    }
    kill(browser);
    None
}

pub fn kill(mut browser: Child) {
    let _ = Command::new("kill")
        .args(["-KILL", &format!("-{}", browser.id())])
        .status();
    let _ = browser.wait();
    std::thread::sleep(Duration::from_millis(300));
}

/// The pages the browser of `profile` has open, as its DevTools lists them.
pub fn list_pages(profile: &Path) -> String {
    use std::io::{Read, Write};
    let port = read_active_port(profile).unwrap().port;
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
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
