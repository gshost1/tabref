//! Relative paths in a command line mean the client's working directory, and
//! the daemon runs in `/`. So the client resolves every argument that names
//! a local file itself, exactly as an in-process run would, and sends the
//! results; the daemon puts them back into the parsed command. A new command
//! that takes a file path must be listed here.

use std::path::PathBuf;

use crate::cli::command::Command;
use crate::commands::open::normalize_url;
use crate::commands::screenshot_path::default_out;

/// The command's path arguments, resolved in this process's directory: the
/// address `open` navigates to, its download folder and its launch profile
/// (each in its own place, empty when not given), the files `upload` sets,
/// the file `screenshot` writes (its default name made here).
/// `None` for a command that takes no path.
pub(crate) fn resolved_paths(command: &Command) -> Option<Vec<String>> {
    match command {
        Command::Open {
            url,
            download_dir,
            profile,
            ..
        } => {
            let slot =
                |dir: &Option<PathBuf>| dir.as_deref().map(absolute_text).unwrap_or_default();
            let mut paths = vec![normalize_url(url), slot(download_dir), slot(profile)];
            while paths.last().is_some_and(String::is_empty) {
                paths.pop();
            }
            Some(paths)
        }
        Command::Upload { files, .. } => {
            Some(files.iter().map(|file| absolute_text(file)).collect())
        }
        Command::Screenshot { out, .. } => Some(vec![match out {
            Some(out) => absolute_text(out),
            None => absolute_text(&default_out(&std::env::current_dir().unwrap_or_default())),
        }]),
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
        Command::Open {
            url,
            download_dir,
            profile,
            ..
        } => {
            let mut paths = paths.into_iter();
            if let Some(resolved) = paths.next() {
                *url = resolved;
            }
            for slot in [download_dir, profile] {
                if let Some(dir) = paths.next().filter(|dir| !dir.is_empty()) {
                    *slot = Some(PathBuf::from(dir));
                }
            }
        }
        Command::Upload { files, .. } => {
            *files = paths.into_iter().map(PathBuf::from).collect();
        }
        Command::Screenshot { out, .. } => {
            if let Some(path) = paths.into_iter().next() {
                *out = Some(PathBuf::from(path));
            }
        }
        _ => {}
    }
}

fn absolute_text(path: &std::path::Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
#[path = "request_paths_tests.rs"]
mod tests;
