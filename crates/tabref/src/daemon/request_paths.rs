//! Relative paths in a command line mean the client's working directory, not
//! the daemon's. Before the daemon runs a command, every argument that names
//! a local file is made absolute against the directory the client sent. A
//! new command that takes a file path must be listed here.

use std::path::Path;

use crate::cli::command::Command;

pub(crate) fn absolutize(command: &mut Command, cwd: &Path) {
    match command {
        Command::Upload { files, .. } => {
            for file in files.iter_mut() {
                if file.is_relative() {
                    *file = cwd.join(&*file);
                }
            }
        }
        Command::Open { url, .. } => {
            let candidate = Path::new(url.trim());
            if !url.contains("://") && candidate.is_relative() && cwd.join(candidate).is_file() {
                *url = cwd.join(candidate).to_string_lossy().into_owned();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "request_paths_tests.rs"]
mod tests;
