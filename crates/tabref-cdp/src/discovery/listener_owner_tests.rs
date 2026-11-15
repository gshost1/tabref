#![cfg(target_os = "linux")]

use std::os::unix::fs::{MetadataExt, symlink};
use std::path::{Path, PathBuf};

use super::{ListenerVerdict, ProcessView, check_in, is_chrome_family};

const HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
const V6_HEADER: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";

/// A throwaway `/proc` with the TCP tables and processes a test lays out.
struct FakeProc {
    root: PathBuf,
}

impl FakeProc {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tabref-fakeproc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("net")).unwrap();
        std::fs::write(root.join("net/tcp"), HEADER).unwrap();
        Self { root }
    }

    fn listen_v4(&self, address: &str, port: u16, state: &str, inode: u64) -> &Self {
        let line = format!(
            "   0: {address}:{port:04X} 00000000:0000 {state} 00000000:00000000 00:00000000 00000000  1000        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
        );
        append(&self.root.join("net/tcp"), &line);
        self
    }

    fn listen_v6(&self, address: &str, port: u16, inode: u64) -> &Self {
        let path = self.root.join("net/tcp6");
        if !path.exists() {
            std::fs::write(&path, V6_HEADER).unwrap();
        }
        let line = format!(
            "   0: {address}:{port:04X} 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 {inode} 1 0000000000000000 100 0 0 10 0\n"
        );
        append(&path, &line);
        self
    }

    fn process(&self, pid: u32, exe: &str, args: &[&str], sockets: &[u64]) -> &Self {
        let dir = self.root.join(pid.to_string());
        std::fs::create_dir_all(dir.join("fd")).unwrap();
        symlink(exe, dir.join("exe")).unwrap();
        let mut cmdline = args.join("\0");
        cmdline.push('\0');
        std::fs::write(dir.join("cmdline"), cmdline).unwrap();
        symlink("/dev/null", dir.join("fd/0")).unwrap();
        for (n, inode) in sockets.iter().enumerate() {
            symlink(
                format!("socket:[{inode}]"),
                dir.join(format!("fd/{}", n + 3)),
            )
            .unwrap();
        }
        self
    }

    fn check(&self, port: u16, trusted: &dyn Fn(&ProcessView) -> bool) -> ListenerVerdict {
        let uid = std::fs::metadata(&self.root).unwrap().uid();
        check_in(&self.root, port, uid, trusted)
    }
}

impl Drop for FakeProc {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn append(path: &Path, line: &str) {
    let mut text = std::fs::read_to_string(path).unwrap();
    text.push_str(line);
    std::fs::write(path, text).unwrap();
}

const CHROME: &str = "/opt/google/chrome/chrome";
const LOOPBACK: &str = "0100007F";

#[test]
fn nothing_listening_is_nothing() {
    let proc = FakeProc::new("nothing");
    proc.listen_v4(LOOPBACK, 9333, "0A", 70)
        .listen_v4(LOOPBACK, 9222, "01", 71)
        .process(10, CHROME, &["chrome"], &[70, 71]);
    assert_eq!(
        proc.check(9222, &is_chrome_family),
        ListenerVerdict::Nothing
    );
}

#[test]
fn a_chrome_of_this_user_holding_the_port_is_trusted() {
    let proc = FakeProc::new("chrome");
    proc.listen_v4(LOOPBACK, 9222, "0A", 555)
        .process(10, "/usr/bin/bash", &["bash"], &[])
        .process(
            11,
            CHROME,
            &["chrome", "--remote-debugging-port=9222"],
            &[555],
        );
    assert_eq!(
        proc.check(9222, &is_chrome_family),
        ListenerVerdict::Trusted
    );
}

#[test]
fn a_browser_binary_replaced_by_an_update_still_counts() {
    let proc = FakeProc::new("deleted");
    proc.listen_v4(LOOPBACK, 9222, "0A", 555).process(
        11,
        "/opt/google/chrome/chrome (deleted)",
        &["chrome"],
        &[555],
    );
    assert_eq!(
        proc.check(9222, &is_chrome_family),
        ListenerVerdict::Trusted
    );
}

#[test]
fn another_program_of_this_user_on_the_port_is_untrusted() {
    let proc = FakeProc::new("python");
    proc.listen_v4(LOOPBACK, 9222, "0A", 555).process(
        12,
        "/usr/bin/python3.14",
        &["python3", "-m", "http.server", "9222"],
        &[555],
    );
    assert!(matches!(
        proc.check(9222, &is_chrome_family),
        ListenerVerdict::Untrusted(_)
    ));
}

#[test]
fn a_listener_no_process_of_this_user_holds_is_untrusted() {
    let proc = FakeProc::new("foreign");
    proc.listen_v4(LOOPBACK, 9222, "0A", 555)
        .process(11, CHROME, &["chrome"], &[777]);
    assert!(matches!(
        proc.check(9222, &is_chrome_family),
        ListenerVerdict::Untrusted(_)
    ));
}

#[test]
fn every_socket_that_could_take_the_connection_must_be_trusted() {
    for (v4, v6) in [
        ("00000000", None),
        (LOOPBACK, Some("00000000000000000000000000000000")),
        (LOOPBACK, Some("0000000000000000FFFF00000100007F")),
    ] {
        let proc = FakeProc::new("wildcard");
        proc.listen_v4(LOOPBACK, 9222, "0A", 555)
            .process(11, CHROME, &["chrome"], &[555])
            .process(12, "/usr/bin/nc", &["nc", "-l", "9222"], &[556, 557]);
        if v4 != LOOPBACK {
            proc.listen_v4(v4, 9222, "0A", 556);
        }
        if let Some(v6) = v6 {
            proc.listen_v6(v6, 9222, 557);
        }
        assert!(
            matches!(
                proc.check(9222, &is_chrome_family),
                ListenerVerdict::Untrusted(_)
            ),
            "{v4} {v6:?}"
        );
    }
}

#[test]
fn a_listener_on_ipv6_loopback_alone_cannot_take_an_ipv4_connection() {
    let proc = FakeProc::new("v6only");
    proc.listen_v6("00000000000000000000000001000000", 9222, 557)
        .process(12, "/usr/bin/nc", &["nc"], &[557]);
    assert_eq!(
        proc.check(9222, &is_chrome_family),
        ListenerVerdict::Nothing
    );
}

#[test]
fn a_launched_browser_is_known_by_its_profile_not_its_binary_name() {
    let proc = FakeProc::new("launched");
    proc.listen_v4(LOOPBACK, 40123, "0A", 900).process(
        30,
        "/home/u/bin/my-chromium-build",
        &["my-chromium-build", "--user-data-dir=/tmp/tabref-profile-a"],
        &[900],
    );
    let ours = |p: &ProcessView| {
        p.cmdline
            .iter()
            .any(|arg| arg == "--user-data-dir=/tmp/tabref-profile-a")
    };
    let other = |p: &ProcessView| {
        p.cmdline
            .iter()
            .any(|arg| arg == "--user-data-dir=/tmp/tabref-profile-b")
    };
    assert_eq!(proc.check(40123, &ours), ListenerVerdict::Trusted);
    assert!(matches!(
        proc.check(40123, &other),
        ListenerVerdict::Untrusted(_)
    ));
}

#[test]
fn chrome_family_binaries_are_recognised_and_others_are_not() {
    let view = |exe: &str| ProcessView {
        pid: 1,
        exe: PathBuf::from(exe),
        cmdline: Vec::new(),
    };
    for exe in [
        CHROME,
        "/usr/lib/chromium/chromium",
        "/snap/chromium/3000/usr/lib/chromium-browser/chrome",
        "/opt/microsoft/msedge/msedge",
        "/opt/brave.com/brave/brave",
        "/home/u/.cache/ms-playwright/chromium_headless_shell-1200/chrome-linux/headless_shell",
        "/x/chrome-headless-shell",
    ] {
        assert!(is_chrome_family(&view(exe)), "{exe}");
    }
    for exe in [
        "/usr/bin/python3",
        "/usr/bin/nc",
        "/tmp/chrome-ish",
        "/usr/bin/node",
    ] {
        assert!(!is_chrome_family(&view(exe)), "{exe}");
    }
}
