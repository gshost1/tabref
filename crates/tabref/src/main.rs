//! `tabref`: a command-line tool an AI agent calls to read and drive Chrome
//! over the DevTools protocol. Each invocation runs one command against one
//! session and prints compact text, or the JSON envelope with `--json`.

mod cli;
mod commands;
mod output;
mod session;
mod view;

use std::process::ExitCode;

use clap::Parser;

use cli::Cli;
use output::emit::{emit_error, emit_ok};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let name = cli.command.name();
    match commands::run(&cli) {
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
