//! Who owns a loopback DevTools port, checked before tabref sends it a single
//! byte. A port answering like DevTools proves nothing: any local program can
//! listen on 9222 or on a port a browser used to hold. On Linux, every socket
//! that could take a connection to `127.0.0.1:<port>` (listening on
//! 127.0.0.1, 0.0.0.0, `::` or `::ffff:127.0.0.1`) is found in
//! `/proc/net/tcp{,6}` and matched by inode against the open sockets of this
//! user's processes the caller trusts; one socket no trusted process holds
//! makes the port untrusted. Elsewhere the answer is `Unknown`.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::listener_verdict::ListenerVerdict;
use super::process_view::ProcessView;

const LISTEN: &str = "0A";
const V4_REACHABLE: [&str; 2] = ["0100007F", "00000000"];
const V6_REACHABLE: [&str; 2] = [
    "00000000000000000000000000000000",
    "0000000000000000FFFF00000100007F",
];
const CHROME_FAMILY: &[&str] = &[
    "chrome",
    "chromium",
    "chromium-browser",
    "chrome-headless-shell",
    "headless_shell",
    "msedge",
    "brave",
];

/// Who holds `port` on this machine, with `trusted` saying which of this
/// user's processes may.
pub fn check_listener(port: u16, trusted: &dyn Fn(&ProcessView) -> bool) -> ListenerVerdict {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let proc_root = Path::new("/proc");
        match std::fs::metadata(proc_root.join("self")) {
            Ok(own) => check_in(proc_root, port, own.uid(), trusted),
            Err(_) => ListenerVerdict::Untrusted("/proc/self could not be read".to_owned()),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (port, trusted);
        ListenerVerdict::Unknown
    }
}

/// Whether the process runs a Chrome-family browser binary.
pub fn is_chrome_family(process: &ProcessView) -> bool {
    CHROME_FAMILY.contains(&process.exe_name().as_str())
}

/// [`check_listener`] against the `/proc` at `proc_root`, for the user `uid`.
#[cfg(unix)]
pub(crate) fn check_in(
    proc_root: &Path,
    port: u16,
    uid: u32,
    trusted: &dyn Fn(&ProcessView) -> bool,
) -> ListenerVerdict {
    let mut unclaimed = match listening_inodes(proc_root, port) {
        Ok(inodes) => inodes,
        Err(what) => return ListenerVerdict::Untrusted(what),
    };
    if unclaimed.is_empty() {
        return ListenerVerdict::Nothing;
    }
    let Ok(entries) = std::fs::read_dir(proc_root) else {
        return ListenerVerdict::Untrusted("the process list could not be read".to_owned());
    };
    for entry in entries.flatten() {
        let Some(view) = own_process(&entry.path(), uid) else {
            continue;
        };
        if !trusted(&view) {
            continue;
        }
        for inode in socket_inodes(&entry.path()) {
            unclaimed.remove(&inode);
        }
        if unclaimed.is_empty() {
            return ListenerVerdict::Trusted;
        }
    }
    ListenerVerdict::Untrusted(format!(
        "port {port} is held by a process that is not a trusted browser of this user"
    ))
}

#[cfg(unix)]
fn own_process(dir: &Path, uid: u32) -> Option<ProcessView> {
    use std::os::unix::fs::MetadataExt;
    let pid = dir.file_name()?.to_str()?.parse::<u32>().ok()?;
    if std::fs::metadata(dir).ok()?.uid() != uid {
        return None;
    }
    let exe = std::fs::read_link(dir.join("exe")).unwrap_or_else(|_| PathBuf::new());
    let cmdline = std::fs::read(dir.join("cmdline"))
        .map(|raw| {
            raw.split(|byte| *byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect()
        })
        .unwrap_or_default();
    Some(ProcessView { pid, exe, cmdline })
}

fn socket_inodes(dir: &Path) -> Vec<u64> {
    let Ok(fds) = std::fs::read_dir(dir.join("fd")) else {
        return Vec::new();
    };
    fds.flatten()
        .filter_map(|fd| std::fs::read_link(fd.path()).ok())
        .filter_map(|target| {
            let target = target.to_string_lossy().into_owned();
            target
                .strip_prefix("socket:[")?
                .strip_suffix(']')?
                .parse::<u64>()
                .ok()
        })
        .collect()
}

/// The inodes of every socket listening where a connection to
/// `127.0.0.1:<port>` could land.
fn listening_inodes(proc_root: &Path, port: u16) -> Result<HashSet<u64>, String> {
    let v4 = std::fs::read_to_string(proc_root.join("net/tcp"))
        .map_err(|_| "the TCP socket table could not be read".to_owned())?;
    let mut inodes = reachable(&v4, port, &V4_REACHABLE);
    if let Ok(v6) = std::fs::read_to_string(proc_root.join("net/tcp6")) {
        inodes.extend(reachable(&v6, port, &V6_REACHABLE));
    }
    Ok(inodes)
}

fn reachable(table: &str, port: u16, addresses: &[&str]) -> HashSet<u64> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (address, raw_port) = fields.get(1)?.rsplit_once(':')?;
            let listening = fields.get(3) == Some(&LISTEN)
                && u16::from_str_radix(raw_port, 16).ok() == Some(port)
                && addresses
                    .iter()
                    .any(|reach| reach.eq_ignore_ascii_case(address));
            listening
                .then(|| fields.get(9)?.parse::<u64>().ok())
                .flatten()
        })
        .collect()
}

#[cfg(test)]
#[path = "listener_owner_tests.rs"]
mod tests;
