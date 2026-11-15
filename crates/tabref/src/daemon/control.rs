use serde_json::json;
use tabref_core::AppError;

use crate::cli::Cli;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// `tabref daemon start|status|stop` for the daemon that serves this state
/// root and endpoint.
pub(crate) fn run(cli: &Cli, action: &str) -> Result<Outcome, AppError> {
    imp::run(cli.cdp.as_deref(), action)
}

/// Stops the daemon that serves this state root without an endpoint (the
/// one that may use the user's own browser), whatever `--cdp` says, and
/// waits until its process has exited: a daemon that only removed its socket
/// may still be running a command. Returns the pid that was running, if
/// any; an error when it did not exit in time.
pub(crate) fn stop_endpoint_less() -> Result<Option<u32>, AppError> {
    let status = imp::run(None, "status")?;
    let pid = (status.data["running"] == json!(true))
        .then(|| status.data["pid"].as_u64())
        .flatten()
        .and_then(|pid| u32::try_from(pid).ok());
    imp::run(None, "stop")?;
    if let Some(pid) = pid {
        imp::wait_for_exit(pid)?;
    }
    Ok(pid)
}

/// The daemon command never runs against a browser session; the client
/// handles it before one is opened.
pub(crate) fn run_in_session(_session: &mut Session, action: &str) -> Result<Outcome, AppError> {
    Err(AppError::invalid_args(format!(
        "'daemon {action}' is handled by the client, not inside a session"
    )))
}

fn not_running() -> Outcome {
    Outcome::new(
        "no daemon running for this endpoint",
        json!({ "running": false }),
    )
}

#[cfg(unix)]
mod imp {
    use std::time::Duration;

    use tabref_core::AppError;

    use super::not_running;
    use crate::daemon::build_id::build_id;
    use crate::daemon::client::{Exchange, exchange};
    use crate::daemon::daemon_address::DaemonAddress;
    use crate::daemon::spawn::spawn_daemon;
    use crate::daemon::wire_request::WireRequest;
    use crate::output::outcome::Outcome;

    const CONTROL_WAIT: Duration = Duration::from_millis(5_000);
    const EXIT_WAIT: Duration = Duration::from_secs(10);

    /// Waits until process `pid` no longer exists.
    pub(super) fn wait_for_exit(pid: u32) -> Result<(), AppError> {
        let started = std::time::Instant::now();
        let target = i32::try_from(pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw);
        while target.is_some_and(|target| rustix::process::test_kill_process(target).is_ok()) {
            if started.elapsed() > EXIT_WAIT {
                return Err(AppError::internal(format!(
                    "The tabref daemon (pid {pid}) did not exit after it was stopped"
                )));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }

    pub(super) fn run(cdp: Option<&str>, action: &str) -> Result<Outcome, AppError> {
        let address = DaemonAddress::for_endpoint(cdp)?;
        let op = if action == "stop" { "stop" } else { "status" };
        let request = WireRequest::control(op, build_id());
        for attempt in 0..2 {
            match exchange(&address, &request, CONTROL_WAIT) {
                Exchange::Reply(reply) if reply.replaced && op == "stop" => {
                    return Ok(Outcome::new(
                        "daemon stopped",
                        serde_json::json!({ "stopped": true }),
                    ));
                }
                Exchange::Reply(reply) if !reply.replaced => return reply.into_result(),
                Exchange::Lost(error) => return Err(error),
                Exchange::Reply(_) | Exchange::NoDaemon => {
                    if action != "start" || attempt > 0 {
                        return Ok(not_running());
                    }
                    if !spawn_daemon(&address, cdp) {
                        return Err(AppError::internal("The tabref daemon could not start"));
                    }
                }
            }
        }
        Ok(not_running())
    }
}

#[cfg(not(unix))]
mod imp {
    use serde_json::json;
    use tabref_core::AppError;

    use crate::output::outcome::Outcome;

    pub(super) fn run(_cdp: Option<&str>, _action: &str) -> Result<Outcome, AppError> {
        Ok(Outcome::new(
            "the daemon is not available on this platform; every command runs in-process",
            json!({ "running": false, "supported": false }),
        ))
    }

    pub(super) fn wait_for_exit(_pid: u32) -> Result<(), AppError> {
        Ok(())
    }
}
