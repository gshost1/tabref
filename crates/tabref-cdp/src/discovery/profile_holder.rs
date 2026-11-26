//! Whether a process really runs a profile. A profile's `SingletonLock` a
//! crash left behind names a pid the system may later give to the browser of
//! another profile, so the lock alone can point at the wrong browser. A
//! running Chrome-family browser keeps files open under its own user-data
//! directory (its databases and logs), so the process's open files settle
//! it: Linux reads `/proc/<pid>/fd`, macOS asks `lsof -p`. Only the names of
//! the open files are read, never their contents.

use std::path::Path;

/// Whether process `pid` has a file open under `user_data_dir`: `None`
/// when this platform cannot say (Windows, or `lsof` did not finish).
pub fn holds_profile(pid: u32, user_data_dir: &Path) -> Option<bool> {
    let dir = std::fs::canonicalize(user_data_dir).ok()?;
    let open = open_files(pid)?;
    Some(open.iter().any(|path| Path::new(path).starts_with(&dir)))
}

#[cfg(target_os = "linux")]
fn open_files(pid: u32) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(format!("/proc/{pid}/fd")).ok()?;
    Some(
        entries
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .map(|target| target.to_string_lossy().into_owned())
            .collect(),
    )
}

#[cfg(target_os = "macos")]
fn open_files(pid: u32) -> Option<Vec<String>> {
    let text =
        super::lsof_owner::run_lsof(&["-b", "-w", "-nP", "-a", "-p", &pid.to_string(), "-Fn"])?;
    Some(parse_lsof_names(&text))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn open_files(pid: u32) -> Option<Vec<String>> {
    let _ = pid;
    None
}

/// The file names in `lsof -Fn` output (`n<name>` lines).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn parse_lsof_names(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
#[path = "profile_holder_tests.rs"]
mod tests;
