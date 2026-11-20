//! Who owns a loopback DevTools port, checked before tabref sends it a single
//! byte. A port answering like DevTools proves nothing: any local program can
//! listen on 9222 or on a port a browser used to hold. On Linux, every socket
//! listening on the port, on any address but `[::1]` (which cannot take the
//! IPv4 connection tabref makes), is found in `/proc/net/tcp{,6}` and matched
//! by inode against the open sockets of this user's processes. The port is
//! trusted only when trusted processes hold every one of those sockets and no
//! other process holds any of them; a socket no visible process holds belongs
//! to another user or to root. Tables that cannot be read are `CantCheck`;
//! other platforms are `Unknown`.
#![cfg_attr(not(target_os = "linux"), allow(dead_code, unused_imports))]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::held_port::HeldPort;
use super::listener_verdict::ListenerVerdict;
use super::process_view::ProcessView;

const LISTEN: &str = "0A";
const V6_LOOPBACK: &str = "00000000000000000000000001000000";
const MAX_NAME: usize = 64;
const CHROME_FAMILY: &[&str] = &[
    "chrome",
    "chromium",
    "chromium-browser",
    "chrome-headless-shell",
    "headless_shell",
    "msedge",
    "brave",
    "vivaldi-bin",
    "opera",
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
            Err(_) => ListenerVerdict::CantCheck("/proc/self could not be read".to_owned()),
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
    let sockets = match listening_sockets(proc_root, port) {
        Ok(sockets) => sockets,
        Err(what) => return ListenerVerdict::CantCheck(what),
    };
    if sockets.is_empty() {
        return ListenerVerdict::Nothing;
    }
    if sockets.iter().any(|(_, owner)| *owner != uid) {
        return ListenerVerdict::Untrusted { holder: None };
    }
    let listening: BTreeSet<u64> = sockets.into_iter().map(|(inode, _)| inode).collect();
    let Ok(entries) = std::fs::read_dir(proc_root) else {
        return ListenerVerdict::CantCheck("the process list could not be read".to_owned());
    };
    let mut claimed = BTreeSet::new();
    let mut stranger = None;
    for entry in entries.flatten() {
        let Some(view) = own_process(&entry.path(), uid) else {
            continue;
        };
        let held: Vec<u64> = socket_inodes(&entry.path())
            .into_iter()
            .filter(|inode| listening.contains(inode))
            .collect();
        if held.is_empty() {
            continue;
        }
        if trusted(&view) {
            claimed.extend(held);
        } else if stranger.is_none() {
            stranger = Some(describe(&view));
        }
    }
    if stranger.is_some() || claimed != listening {
        return ListenerVerdict::Untrusted { holder: stranger };
    }
    ListenerVerdict::Trusted(HeldPort {
        port,
        inodes: listening,
        proc_root: proc_root.to_path_buf(),
    })
}

/// A process as an error names it: `"python3" (pid 1234)`. The name comes
/// from a binary anyone can name, so it is quoted, stripped of control
/// characters (a newline would forge a line of the message) and cut to 64.
fn describe(view: &ProcessView) -> String {
    let name = view.exe_name();
    let name = if name.is_empty() {
        view.cmdline
            .first()
            .and_then(|arg| Path::new(arg).file_name())
            .map_or_else(
                || "a process".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            )
    } else {
        name
    };
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .take(MAX_NAME)
        .collect();
    format!("\"{name}\" (pid {})", view.pid)
}

/// The process `/proc/<pid>` at `dir` describes, when it is `uid`'s.
#[cfg(unix)]
pub(crate) fn own_process(dir: &Path, uid: u32) -> Option<ProcessView> {
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

/// The inodes of every socket listening on `port`, on any address but
/// `[::1]`.
pub(crate) fn listening_inodes(proc_root: &Path, port: u16) -> Result<BTreeSet<u64>, String> {
    Ok(listening_sockets(proc_root, port)?
        .into_iter()
        .map(|(inode, _)| inode)
        .collect())
}

/// Every socket listening on `port`, on any address but `[::1]`, as its
/// inode and the uid that owns it.
fn listening_sockets(proc_root: &Path, port: u16) -> Result<Vec<(u64, u32)>, String> {
    let v4 = std::fs::read_to_string(proc_root.join("net/tcp"))
        .map_err(|_| "the TCP socket table could not be read".to_owned())?;
    let mut sockets = listeners(&v4, port, None);
    match std::fs::read_to_string(proc_root.join("net/tcp6")) {
        Ok(v6) => sockets.extend(listeners(&v6, port, Some(V6_LOOPBACK))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("the TCP6 socket table could not be read".to_owned()),
    }
    Ok(sockets)
}

fn listeners(table: &str, port: u16, except: Option<&str>) -> Vec<(u64, u32)> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (address, raw_port) = fields.get(1)?.rsplit_once(':')?;
            let listening = fields.get(3) == Some(&LISTEN)
                && u16::from_str_radix(raw_port, 16).ok() == Some(port)
                && except.is_none_or(|skip| !skip.eq_ignore_ascii_case(address));
            if !listening {
                return None;
            }
            Some((fields.get(9)?.parse().ok()?, fields.get(7)?.parse().ok()?))
        })
        .collect()
}

#[cfg(test)]
#[path = "listener_owner_tests.rs"]
mod tests;
