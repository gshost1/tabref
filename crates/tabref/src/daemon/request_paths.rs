//! Relative paths in a command line mean the client's working directory, and
//! the daemon runs in `/`. So the client resolves every argument that names
//! a local file itself, exactly as an in-process run would, and sends the
//! results; the daemon puts them back into the parsed command. A new command
//! that takes a file path must be listed here.

use std::path::PathBuf;

use crate::cli::command::Command;
use crate::commands::open::normalize_url;

/// The command's path arguments, resolved in this process's directory: the
/// address `open` navigates to, the files `upload` sets. `None` for a
/// command that takes no path.
pub(crate) fn resolved_paths(command: &Command) -> Option<Vec<String>> {
    match command {
        Command::Open { url, profile, .. } => Some(
            std::iter::once(normalize_url(url))
                .chain(profile.iter().map(|dir| {
                    std::path::absolute(dir)
                        .unwrap_or_else(|_| dir.clone())
                        .to_string_lossy()
                        .into_owned()
                }))
                .collect(),
        ),
        Command::Upload { files, .. } => Some(
            files
                .iter()
                .map(|file| {
                    std::path::absolute(file)
                        .unwrap_or_else(|_| file.clone())
                        .to_string_lossy()
                        .into_owned()
                })
                .collect(),
        ),
        _ => None,
    }
}

/// Whether the command must run in-process to report its error as typed:
/// an upload naming a file that is not there.
pub(crate) fn runs_in_process(command: &Command) -> bool {
    matches!(command, Command::Upload { files, .. } if files.iter().any(|file| !file.is_file()))
}

/// Puts the client's resolved paths back into the parsed command.
pub(crate) fn apply_paths(command: &mut Command, paths: Option<Vec<String>>) {
    let Some(paths) = paths else {
        return;
    };
    match command {
        Command::Open { url, profile, .. } => {
            let mut paths = paths.into_iter();
            if let Some(resolved) = paths.next() {
                *url = resolved;
            }
            if let Some(resolved) = paths.next() {
                *profile = Some(PathBuf::from(resolved));
            }
        }
        Command::Upload { files, .. } => {
            *files = paths.into_iter().map(PathBuf::from).collect();
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "request_paths_tests.rs"]
mod tests;
