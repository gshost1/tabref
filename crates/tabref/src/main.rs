//! `tabref`: a command-line tool an AI agent calls to read and drive Chrome
//! over the DevTools protocol. Each invocation runs one command and prints
//! compact text, or the JSON envelope with `--json`. Commands go through a
//! background daemon that keeps the browser connection open; when it cannot
//! run, the command runs in-process with the same output.

mod cli;
mod commands;
mod daemon;
mod output;
mod session;
mod view;

use std::process::ExitCode;

use clap::Parser;

use cli::Cli;
use cli::command::Command;
use output::emit::{emit_error, emit_ok};

fn main() -> ExitCode {
    if daemon::is_daemon_process() {
        return daemon::serve();
    }
    let cli = Cli::parse();
    let name = cli.command.name();
    let result = match &cli.command {
        Command::Daemon { action } => daemon::control::run(&cli, action),
        Command::Connect {
            user_chrome,
            user_data_dir,
            off,
        } => commands::connect::run(&cli, *user_chrome, user_data_dir.as_deref(), *off),
        Command::Skill { .. } => commands::run(&cli),
        _ => daemon::run_through_daemon(&cli).unwrap_or_else(|| commands::run(&cli)),
    };
    match result {
        Ok(outcome) => {
            emit_ok(name, outcome, cli.json);
            ExitCode::SUCCESS
        }
        Err(error) => {
            emit_error(name, &error, cli.json);
            ExitCode::FAILURE
        }
    }
}
