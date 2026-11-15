//! A command that reads its standard input (a batch of steps piped in) gets
//! it through the daemon too: the client reads it and sends it with the
//! request. Only such a command reads stdin; a terminal is never read; and a
//! pipe that stays open, or holds more than fits a request, is an error
//! rather than a hang.

use std::io::{IsTerminal, Read};
use std::sync::mpsc;
use std::time::Duration;

use tabref_core::AppError;

use crate::cli::command::Command;

/// What a request may carry as stdin, leaving room in the 1 MiB request for
/// the rest of it.
const MAX_STDIN: u64 = 960 * 1024;
const STDIN_WAIT: Duration = Duration::from_secs(2);

/// Whether `command` reads its standard input. None does yet; a command
/// that takes its steps from stdin when none are given inline answers true
/// for exactly that case.
pub(crate) fn takes_stdin(command: &Command) -> bool {
    let _ = command;
    false
}

/// The stdin to send with `command`, when it reads one.
pub(crate) fn request_stdin(command: &Command) -> Result<Option<String>, AppError> {
    if !takes_stdin(command) || std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = std::io::stdin()
            .lock()
            .take(MAX_STDIN + 1)
            .read_to_end(&mut bytes);
        let _ = sender.send(read.map(|_| bytes));
    });
    let bytes = match receiver.recv_timeout(STDIN_WAIT) {
        Ok(read) => read?,
        Err(_) => {
            return Err(AppError::invalid_args(
                "Standard input stayed open; close it (end the heredoc) or pass the steps inline",
            ));
        }
    };
    if bytes.len() as u64 > MAX_STDIN {
        return Err(AppError::invalid_args(format!(
            "Standard input is larger than {} KiB",
            MAX_STDIN / 1024
        )));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| AppError::invalid_args("Standard input is not UTF-8 text"))
}
