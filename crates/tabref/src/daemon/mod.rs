//! The background daemon that keeps one browser connection per state root
//! and endpoint alive across commands, and the thin client that talks to it
//! over a Unix socket. On other platforms, and whenever the daemon cannot
//! start, every command runs in-process with the same output.
#![cfg_attr(not(unix), allow(dead_code))]

pub(crate) mod build_id;
pub(crate) mod control;
pub(crate) mod daemon_address;
pub(crate) mod request_paths;
pub(crate) mod request_stdin;
pub(crate) mod tab_locks;
pub(crate) mod wire_error;
pub(crate) mod wire_reply;
pub(crate) mod wire_request;

#[cfg(unix)]
pub(crate) mod activity;
#[cfg(unix)]
pub(crate) mod client;
#[cfg(unix)]
pub(crate) mod private_dir;
#[cfg(unix)]
pub(crate) mod run_request;
#[cfg(unix)]
pub(crate) mod server;
#[cfg(unix)]
pub(crate) mod server_connection;
#[cfg(unix)]
pub(crate) mod server_state;
#[cfg(unix)]
pub(crate) mod spawn;
#[cfg(unix)]
pub(crate) mod stale_sweep;
#[cfg(unix)]
pub(crate) mod user_browser_turn;
#[cfg(unix)]
pub(crate) mod wire_io;

use tabref_core::AppError;

use crate::cli::Cli;
use crate::output::outcome::Outcome;

/// Set to `1` in the environment of the daemon process.
pub(crate) const DAEMON_ENV: &str = "TABREF_DAEMON";
/// The endpoint the daemon serves, as the client resolved it.
pub(crate) const CDP_ENV: &str = "TABREF_DAEMON_CDP";
/// The daemon's idle timeout in milliseconds (default ten minutes).
pub(crate) const IDLE_ENV: &str = "TABREF_DAEMON_IDLE_MS";
/// Any non-empty value runs every command in-process.
pub(crate) const NO_DAEMON_ENV: &str = "TABREF_NO_DAEMON";

/// How long the daemon waits without a command before it exits:
/// `TABREF_DAEMON_IDLE_MS`, else ten minutes.
pub(crate) fn idle_timeout() -> std::time::Duration {
    std::env::var(IDLE_ENV)
        .ok()
        .and_then(|ms| ms.parse::<u64>().ok())
        .map_or(
            std::time::Duration::from_secs(600),
            std::time::Duration::from_millis,
        )
}

/// Whether this process was started as a daemon.
pub(crate) fn is_daemon_process() -> bool {
    std::env::var_os(DAEMON_ENV).is_some_and(|value| value == "1")
}

/// Serves requests until the daemon exits.
pub(crate) fn serve() -> std::process::ExitCode {
    #[cfg(unix)]
    {
        server::serve()
    }
    #[cfg(not(unix))]
    {
        std::process::ExitCode::FAILURE
    }
}

/// Runs the command through the daemon; `None` when it must run in-process.
pub(crate) fn run_through_daemon(cli: &Cli) -> Option<Result<Outcome, AppError>> {
    #[cfg(unix)]
    {
        client::run_through_daemon(cli)
    }
    #[cfg(not(unix))]
    {
        let _ = cli;
        None
    }
}
