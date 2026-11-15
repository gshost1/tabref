//! Who holds a TCP listener on macOS, where `/proc` does not exist: the pids
//! `/usr/sbin/lsof -nP -iTCP:<port> -sTCP:LISTEN -Fp` reports (one `p<pid>`
//! line per process, on any address). It costs a process spawn, so only the
//! opt-in attach to the user's own browser uses it.

/// The pids holding a listener on `port`: empty when none does, `None` when
/// this platform cannot say (not macOS, or `lsof` did not run).
pub fn listener_pids(port: u16) -> Option<Vec<u32>> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("/usr/sbin/lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fp"])
            .env("LC_ALL", "C")
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let ran = output.status.success() || (output.status.code() == Some(1) && text.is_empty());
        ran.then(|| parse_lsof_pids(&text))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = port;
        None
    }
}

/// The pids in `lsof -Fp` output.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn parse_lsof_pids(output: &str) -> Vec<u32> {
    let mut pids: Vec<u32> = output
        .lines()
        .filter_map(|line| line.strip_prefix('p')?.trim().parse().ok())
        .collect();
    pids.dedup();
    pids
}

#[cfg(test)]
#[path = "lsof_owner_tests.rs"]
mod tests;
