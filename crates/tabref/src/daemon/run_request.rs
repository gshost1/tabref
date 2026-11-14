use std::path::Path;

use clap::Parser;
use tabref_core::AppError;

use super::request_paths::absolutize;
use super::server_state::ServerState;
use super::wire_request::WireRequest;
use crate::cli::Cli;
use crate::cli::command::Command;
use crate::commands;
use crate::output::outcome::Outcome;

/// Runs one client command line inside the daemon, against a kept browser
/// connection, exactly as `commands::run` would in-process: the same parse,
/// the same session options, the same dispatch.
pub(crate) fn run_request(state: &ServerState, request: WireRequest) -> Result<Outcome, AppError> {
    let argv = std::iter::once("tabref".to_owned()).chain(request.argv);
    let mut cli = Cli::try_parse_from(argv).map_err(|error| {
        AppError::invalid_args(format!("The daemon could not read the command: {error}"))
    })?;
    cli.cdp = request.cdp;
    absolutize(&mut cli.command, Path::new(&request.cwd));
    if matches!(cli.command, Command::Skill) {
        return commands::run(&cli);
    }
    let options = commands::session_options(&cli);
    let mut session = state.checkout(&options)?;
    let outcome = commands::dispatch(&mut session, &cli.command);
    state.checkin(session);
    outcome
}
