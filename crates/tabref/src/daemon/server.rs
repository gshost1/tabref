//! The daemon process: the same binary, started by a client with
//! `TABREF_DAEMON=1`. It takes the folder's lock (so one daemon serves an
//! address, and a dead one never blocks the next), binds the socket, and
//! serves each connection on its own thread. It exits after its idle
//! timeout, on `stop`, when a client of another build asks it to step
//! aside, or once its state root is deleted.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tabref_core::AppError;

use super::daemon_address::DaemonAddress;
use super::private_dir::ensure_private_dir;
use super::server_connection::{peer_is_this_user, serve_connection};
use super::server_state::ServerState;
use super::stale_sweep::sweep_stale_dirs;
use super::{CDP_ENV, IDLE_ENV, build_id::build_id};

const DEFAULT_IDLE: Duration = Duration::from_secs(600);
const LOCK_WAIT: Duration = Duration::from_millis(1_500);
const LOCK_POLL: Duration = Duration::from_millis(5);
const IDLE_CHECK: Duration = Duration::from_millis(500);
const RETIRE_POLL: Duration = Duration::from_millis(20);

pub(crate) fn serve() -> ExitCode {
    let cdp = std::env::var(CDP_ENV).ok().filter(|cdp| !cdp.is_empty());
    let Ok(address) = DaemonAddress::for_endpoint(cdp.as_deref()) else {
        return ExitCode::FAILURE;
    };
    let Ok((listener, _lock)) = bind(&address) else {
        return ExitCode::FAILURE;
    };
    let state = Arc::new(ServerState::new(build_id(), cdp));
    watch_idle(Arc::clone(&state), address.clone(), idle_timeout());
    let sweep = address.clone();
    thread::spawn(move || sweep_stale_dirs(&sweep));
    for stream in listener.incoming().flatten() {
        if !peer_is_this_user(&stream) {
            continue;
        }
        state.activity.begin();
        let state = Arc::clone(&state);
        let address = address.clone();
        thread::spawn(move || {
            if serve_connection(stream, &state) {
                retire(&state, &address);
            }
        });
    }
    ExitCode::SUCCESS
}

/// Takes the folder lock, waiting a moment for a daemon that is stepping
/// aside, then binds the socket under a temporary name, closes it to
/// everyone else and only then moves it into place, so no client ever sees
/// a socket with looser permissions.
fn bind(address: &DaemonAddress) -> Result<(UnixListener, File), AppError> {
    ensure_private_dir(address)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(address.lock_file())?;
    let started = Instant::now();
    while lock.try_lock().is_err() {
        if started.elapsed() > LOCK_WAIT {
            return Err(AppError::internal("Another daemon holds this address"));
        }
        thread::sleep(LOCK_POLL);
    }
    let socket = address.socket();
    let staging = address.dir.join(format!("sock.{}", std::process::id()));
    let _ = std::fs::remove_file(&staging);
    let listener = UnixListener::bind(&staging)?;
    std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o600))?;
    std::fs::rename(&staging, &socket)?;
    Ok((listener, lock))
}

fn watch_idle(state: Arc<ServerState>, address: DaemonAddress, timeout: Duration) {
    thread::spawn(move || {
        loop {
            thread::sleep(IDLE_CHECK.min(timeout));
            state.tend_downloads();
            let orphaned = !address.state_root.exists() && state.activity.idle() > IDLE_CHECK;
            if orphaned || state.activity.idle() >= timeout {
                if orphaned {
                    let _ = std::fs::remove_dir_all(&address.dir);
                }
                retire(&state, &address);
            }
        }
    });
}

/// Stops taking connections by removing the socket, then exits once no
/// request is in flight, including any a client connected just before the
/// socket went away.
fn retire(state: &ServerState, address: &DaemonAddress) {
    let _ = std::fs::remove_file(address.socket());
    state.retire_downloads();
    let mut quiet = 0;
    while quiet < 2 {
        thread::sleep(RETIRE_POLL);
        quiet = if state.activity.active() == 0 {
            quiet + 1
        } else {
            0
        };
    }
    std::process::exit(0);
}

fn idle_timeout() -> Duration {
    std::env::var(IDLE_ENV)
        .ok()
        .and_then(|ms| ms.parse::<u64>().ok())
        .map_or(DEFAULT_IDLE, Duration::from_millis)
}
