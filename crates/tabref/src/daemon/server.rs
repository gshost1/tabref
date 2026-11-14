//! The daemon process: the same binary, started by a client with
//! `TABREF_DAEMON=1`. It takes the folder's lock (so one daemon serves an
//! address, and a dead one never blocks the next), binds the socket, and
//! serves each connection on its own thread: one request line, one reply
//! line. It exits after its idle timeout, on `stop`, when a client of
//! another build asks it to step aside, or once its state root is deleted.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tabref_core::AppError;

use super::daemon_address::DaemonAddress;
use super::private_dir::ensure_private_dir;
use super::run_request::run_request;
use super::server_state::ServerState;
use super::wire_io::{read_line, write_line};
use super::wire_reply::WireReply;
use super::wire_request::WireRequest;
use super::{CDP_ENV, IDLE_ENV, build_id::build_id};
use crate::output::outcome::Outcome;

const DEFAULT_IDLE: Duration = Duration::from_secs(600);
const LOCK_WAIT: Duration = Duration::from_millis(1_500);
const LOCK_POLL: Duration = Duration::from_millis(5);
const IDLE_CHECK: Duration = Duration::from_millis(500);

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
    for stream in listener.incoming().flatten() {
        let state = Arc::clone(&state);
        let address = address.clone();
        thread::spawn(move || serve_connection(stream, &state, &address));
    }
    ExitCode::SUCCESS
}

/// Takes the folder lock, waiting a moment for a daemon that is stepping
/// aside, then replaces whatever socket file a dead daemon left behind.
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
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    Ok((listener, lock))
}

fn serve_connection(mut stream: UnixStream, state: &ServerState, address: &DaemonAddress) {
    let Ok(Some(request)) = read_line::<WireRequest>(&stream) else {
        return;
    };
    let counts = request.op != "status";
    if counts {
        state.activity.begin();
    }
    let (reply, exit) = answer(state, request);
    let _ = write_line(&mut stream, &reply);
    if counts {
        state.activity.end();
    }
    if exit {
        shut_down(address);
    }
}

fn answer(state: &ServerState, request: WireRequest) -> (WireReply, bool) {
    if request.build != state.build {
        return (WireReply::replaced(), true);
    }
    match request.op.as_str() {
        "run" => (WireReply::from_result(&run_request(state, request)), false),
        "status" => (status_reply(state), false),
        "stop" => {
            let stopped = Outcome::new("daemon stopped", serde_json::json!({ "stopped": true }));
            (WireReply::from_result(&Ok(stopped)), true)
        }
        other => {
            let error = AppError::invalid_args(format!("Unknown daemon request '{other}'"));
            (WireReply::from_result(&Err(error)), false)
        }
    }
}

fn status_reply(state: &ServerState) -> WireReply {
    let data = state.status();
    let text = format!(
        "daemon running: pid {}, up {} s, idle {} s, {} connection(s), {} tab(s) attached",
        std::process::id(),
        state.activity.uptime().as_secs(),
        state.activity.idle().as_secs(),
        data["connections"],
        data["attached_tabs"],
    );
    WireReply::from_result(&Ok(Outcome::new(text, data)))
}

fn watch_idle(state: Arc<ServerState>, address: DaemonAddress, timeout: Duration) {
    thread::spawn(move || {
        loop {
            thread::sleep(IDLE_CHECK.min(timeout));
            let orphaned = !address.state_root.exists() && state.activity.idle() > IDLE_CHECK;
            if orphaned {
                let _ = std::fs::remove_dir_all(&address.dir);
                std::process::exit(0);
            }
            if state.activity.idle() >= timeout {
                shut_down(&address);
            }
        }
    });
}

fn shut_down(address: &DaemonAddress) {
    let _ = std::fs::remove_file(address.socket());
    std::process::exit(0);
}

fn idle_timeout() -> Duration {
    std::env::var(IDLE_ENV)
        .ok()
        .and_then(|ms| ms.parse::<u64>().ok())
        .map_or(DEFAULT_IDLE, Duration::from_millis)
}
