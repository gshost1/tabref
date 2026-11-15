use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::daemon_address::DaemonAddress;
use super::private_dir::ensure_private_dir;
use super::{CDP_ENV, DAEMON_ENV};

const READY_WAIT: Duration = Duration::from_millis(3_000);
const FIRST_POLL: Duration = Duration::from_micros(500);
const MAX_POLL: Duration = Duration::from_millis(10);

/// Starts a daemon for `address` from this same binary, in `/` (so it
/// never resolves a path against a client's directory), in its own process
/// group so a terminal's Ctrl-C never reaches it, and waits until its socket
/// accepts. `false` when no daemon came up in time; the caller then runs
/// the command in-process.
pub(crate) fn spawn_daemon(address: &DaemonAddress, cdp: Option<&str>) -> bool {
    if ensure_private_dir(address).is_err() {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let spawned = Command::new(exe)
        .env(DAEMON_ENV, "1")
        .env(CDP_ENV, cdp.unwrap_or_default())
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
    let Ok(mut child) = spawned else {
        return false;
    };
    let ready = wait_until_ready(address, &mut child, READY_WAIT);
    thread::spawn(move || child.wait());
    ready
}

/// Polls the socket until a daemon accepts, quickly at first. A daemon that
/// exits before it is ready (another one holds the address, or it could not
/// bind) ends the wait at once, unless some daemon answers.
fn wait_until_ready(address: &DaemonAddress, child: &mut Child, budget: Duration) -> bool {
    let socket = address.socket();
    let started = Instant::now();
    let mut pause = FIRST_POLL;
    while started.elapsed() < budget {
        if UnixStream::connect(&socket).is_ok() {
            return true;
        }
        if matches!(child.try_wait(), Ok(Some(_))) {
            return UnixStream::connect(&socket).is_ok();
        }
        thread::sleep(pause);
        pause = (pause * 2).min(MAX_POLL);
    }
    false
}
