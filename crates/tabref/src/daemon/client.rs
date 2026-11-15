//! The thin client: one request line over the daemon's socket, one reply
//! line back. Nothing slow sits on this path: no browser probe, no file
//! writes, no sleeps once a daemon is up. A request is retried only when no
//! daemon received it; once it was delivered, a lost reply is an error, not
//! a second run.

use std::os::unix::net::UnixStream;
use std::time::Duration;

use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use super::NO_DAEMON_ENV;
use super::build_id::build_id;
use super::daemon_address::DaemonAddress;
use super::private_dir::{check_private, check_socket};
use super::request_paths::{resolved_paths, runs_in_process};
use super::request_stdin::request_stdin;
use super::spawn::spawn_daemon;
use super::wire_io::{MAX_REPLY, read_line, write_line};
use super::wire_reply::WireReply;
use super::wire_request::WireRequest;
use crate::cli::Cli;
use crate::output::outcome::Outcome;

const REPLY_SLACK: Duration = Duration::from_millis(5_000);
const STEP_ASIDE_WAIT: Duration = Duration::from_millis(1_500);

/// How one exchange with a daemon ended.
pub(crate) enum Exchange {
    Reply(WireReply),
    NoDaemon,
    Lost(AppError),
}

/// Runs the command through the daemon for its endpoint, starting one when
/// none answers. `None` means no daemon can serve it (disabled, or it could
/// not start), and the caller runs the command in-process instead.
pub(crate) fn run_through_daemon(cli: &Cli) -> Option<Result<Outcome, AppError>> {
    if std::env::var_os(NO_DAEMON_ENV).is_some_and(|value| !value.is_empty()) {
        return None;
    }
    let argv = std::env::args_os()
        .skip(1)
        .map(|arg| arg.into_string().ok())
        .collect::<Option<Vec<String>>>()?;
    if runs_in_process(&cli.command) {
        return None;
    }
    let cwd = std::env::current_dir().ok()?.to_str()?.to_owned();
    let address = DaemonAddress::for_endpoint(cli.cdp.as_deref()).ok()?;
    let stdin = match request_stdin(&cli.command) {
        Ok(stdin) => stdin,
        Err(error) => return Some(Err(error)),
    };
    let request = WireRequest {
        op: "run".to_owned(),
        build: build_id(),
        argv,
        cwd,
        cdp: cli.cdp.clone(),
        paths: resolved_paths(&cli.command),
        stdin,
    };
    let wait = Duration::from_millis(cli.timeout).saturating_add(REPLY_SLACK);
    for _ in 0..3 {
        match exchange(&address, &request, wait) {
            Exchange::Reply(reply) if reply.replaced => {
                wait_for_exit(&address);
                if !spawn_daemon(&address, cli.cdp.as_deref()) {
                    return None;
                }
            }
            Exchange::Reply(reply) => return Some(reply.into_result()),
            Exchange::Lost(error) => return Some(Err(error)),
            Exchange::NoDaemon => {
                if !spawn_daemon(&address, cli.cdp.as_deref()) {
                    return None;
                }
            }
        }
    }
    None
}

/// Sends `request` and waits up to `wait` for the reply. A folder or
/// socket that is not this user's own is never connected to; that, or
/// connecting or writing that fails, means no daemon has the request.
pub(crate) fn exchange(address: &DaemonAddress, request: &WireRequest, wait: Duration) -> Exchange {
    let socket = address.socket();
    if check_private(&address.dir).is_err() || check_socket(&socket).is_err() {
        return Exchange::NoDaemon;
    }
    let Ok(mut stream) = UnixStream::connect(&socket) else {
        return Exchange::NoDaemon;
    };
    if write_line(&mut stream, request).is_err() {
        return Exchange::NoDaemon;
    }
    if stream.set_read_timeout(Some(wait)).is_err() {
        return Exchange::Lost(lost("could not bound the wait for its reply"));
    }
    match read_line::<WireReply>(&stream, MAX_REPLY) {
        Ok(Some(reply)) => Exchange::Reply(reply),
        Ok(None) => Exchange::Lost(lost("exited before it replied")),
        Err(error) if is_timeout(&error) => Exchange::Lost(
            AppError::new(
                ErrorCode::Timeout,
                format!(
                    "The tabref daemon did not reply within {} ms",
                    wait.as_millis()
                ),
            )
            .with_suggestion("Run 'tabref daemon stop' and try again")
            .with_details(serde_json::json!({ "kind": "daemon_timeout" }))
            .with_disposition(DeliverySemantics::uncertain()),
        ),
        Err(error) => Exchange::Lost(lost(&error.to_string())),
    }
}

/// Waits until a daemon that is stepping aside has released its socket.
fn wait_for_exit(address: &DaemonAddress) {
    let socket = address.socket();
    let started = std::time::Instant::now();
    while socket.exists() && started.elapsed() < STEP_ASIDE_WAIT {
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

fn lost(detail: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        "The tabref daemon took the command but no reply came back",
    )
    .with_platform_detail(detail)
    .with_suggestion("Check the page with 'tabref snap' before repeating an action")
    .with_details(serde_json::json!({ "kind": "daemon_reply_lost" }))
    .with_disposition(DeliverySemantics::uncertain())
}
