use serde_json::json;
use tabref_core::AppError;

use crate::cli::Cli;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// `tabref daemon start|status|stop` for the daemon that serves this state
/// root and endpoint.
pub(crate) fn run(cli: &Cli, action: &str) -> Result<Outcome, AppError> {
    imp::run(cli, action)
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
    use crate::cli::Cli;
    use crate::daemon::build_id::build_id;
    use crate::daemon::client::{Exchange, exchange};
    use crate::daemon::daemon_address::DaemonAddress;
    use crate::daemon::spawn::spawn_daemon;
    use crate::daemon::wire_request::WireRequest;
    use crate::output::outcome::Outcome;

    const CONTROL_WAIT: Duration = Duration::from_millis(5_000);

    pub(super) fn run(cli: &Cli, action: &str) -> Result<Outcome, AppError> {
        let address = DaemonAddress::for_endpoint(cli.cdp.as_deref())?;
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
                    if !spawn_daemon(&address, cli.cdp.as_deref()) {
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

    use crate::cli::Cli;
    use crate::output::outcome::Outcome;

    pub(super) fn run(_cli: &Cli, _action: &str) -> Result<Outcome, AppError> {
        Ok(Outcome::new(
            "the daemon is not available on this platform; every command runs in-process",
            json!({ "running": false, "supported": false }),
        ))
    }
}
