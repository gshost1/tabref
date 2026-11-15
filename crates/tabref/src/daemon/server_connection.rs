//! One client connection: one request line, one reply line. While the
//! command runs, the connection is watched; a client that goes away (killed,
//! Ctrl-C) cancels its command, which ends its waits and frees its tab.

use std::io::{ErrorKind, Read};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use tabref_core::AppError;

use super::private_dir::effective_uid;
use super::run_request::run_request;
use super::server_state::ServerState;
use super::wire_io::{MAX_REQUEST, read_line, write_line};
use super::wire_reply::WireReply;
use super::wire_request::WireRequest;
use crate::output::outcome::Outcome;

const REQUEST_WAIT: Duration = Duration::from_secs(5);
const REPLY_WAIT: Duration = Duration::from_secs(10);

/// Serves one accepted connection, which the accept loop already counted as
/// active. A request line is at most 1 MiB and must arrive within 5 s; a
/// reply that cannot be written within 10 s is dropped. Returns whether the
/// daemon should now retire.
pub(crate) fn serve_connection(mut stream: UnixStream, state: &ServerState) -> bool {
    let _ = stream.set_read_timeout(Some(REQUEST_WAIT));
    let _ = stream.set_write_timeout(Some(REPLY_WAIT));
    let request = match read_line::<WireRequest>(&stream, MAX_REQUEST) {
        Ok(Some(request)) => request,
        Ok(None) => {
            state.activity.end(false);
            return false;
        }
        Err(error) => {
            let unreadable = error.kind() == ErrorKind::Other;
            if unreadable {
                let _ = write_line(&mut stream, &WireReply::replaced());
            }
            state.activity.end(false);
            return unreadable;
        }
    };
    let cancel = watch_client(&stream);
    let touch = request.op != "status";
    let (reply, retire) = answer(state, request, &cancel);
    let _ = write_line(&mut stream, &reply);
    state.activity.end(touch);
    retire
}

/// Whether the connecting process runs as this daemon's own user. Linux
/// says through `SO_PEERCRED`; elsewhere the private folder and the 0600
/// socket are the only way in.
pub(crate) fn peer_is_this_user(stream: &UnixStream) -> bool {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        rustix::net::sockopt::socket_peercred(stream)
            .is_ok_and(|peer| peer.uid.as_raw() == effective_uid())
    }
    #[cfg(not(any(target_os = "linux", target_os = "android")))]
    {
        let _ = (stream, effective_uid);
        true
    }
}

/// A flag that is set once the client closes its end. The client sends
/// nothing after its request, so any end of input means it went away.
fn watch_client(stream: &UnixStream) -> Arc<AtomicBool> {
    let cancel = Arc::new(AtomicBool::new(false));
    let Ok(mut watched) = stream.try_clone() else {
        return cancel;
    };
    let flag = Arc::clone(&cancel);
    thread::spawn(move || {
        let mut byte = [0_u8; 1];
        loop {
            match watched.read(&mut byte) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) => {}
                Err(_) => break,
            }
        }
        flag.store(true, Ordering::SeqCst);
    });
    cancel
}

/// The reply and whether the daemon retires after it. A client of another
/// build replaces the daemon, except for `status`, which only looks.
fn answer(
    state: &ServerState,
    request: WireRequest,
    cancel: &Arc<AtomicBool>,
) -> (WireReply, bool) {
    if request.op == "status" {
        return (status_reply(state), false);
    }
    if request.build != state.build {
        return (WireReply::replaced(), true);
    }
    match request.op.as_str() {
        "run" => (
            WireReply::from_result(&run_request(state, request, cancel)),
            false,
        ),
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
