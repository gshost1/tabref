use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use clap::Parser;
use tabref_core::AppError;

use super::request_paths::apply_paths;
use super::server_state::ServerState;
use super::wire_request::WireRequest;
use crate::cli::Cli;
use crate::cli::command::Command;
use crate::commands;
use crate::output::outcome::Outcome;

/// Runs one client command line inside the daemon, against a kept browser
/// connection, exactly as `commands::run` would in-process: the same parse,
/// the same session options, the same dispatch.
/// `cancel` is set when the client goes away, which ends the command's
/// waits and frees its tabs.
pub(crate) fn run_request(
    state: &ServerState,
    request: WireRequest,
    cancel: &Arc<AtomicBool>,
) -> Result<Outcome, AppError> {
    let argv = std::iter::once("tabref".to_owned()).chain(request.argv);
    let mut cli = Cli::try_parse_from(argv).map_err(|error| {
        AppError::invalid_args(format!("The daemon could not read the command: {error}"))
    })?;
    cli.cdp = request.cdp;
    apply_paths(&mut cli.command, request.paths);
    if matches!(cli.command, Command::Skill) {
        return commands::run(&cli);
    }
    let mut options = commands::session_options(&cli);
    options.stdin = request.stdin;
    let reads = only_reads(&cli.command);
    if !reads {
        state.captures.clear();
    }
    let mut session = state.checkout(&options, cancel)?;
    if reads {
        session.keep_captures(Arc::clone(&state.captures));
    }
    let outcome = commands::dispatch(&mut session, &cli.command);
    if !reads {
        state.captures.clear();
    }
    let outcome = session.with_warning(outcome);
    state.checkin(session);
    outcome
}

/// Whether `command` only reads the page (or navigates and reads it), so it
/// may keep and reuse a capture. Any other command may change a page in ways
/// no page script sees, such as a hover, so it clears every kept capture.
fn only_reads(command: &Command) -> bool {
    matches!(
        command,
        Command::Open { .. } | Command::Snap { .. } | Command::Find { .. } | Command::Read { .. }
    )
}
