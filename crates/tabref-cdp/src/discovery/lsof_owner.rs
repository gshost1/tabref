//! Who holds a TCP listener on macOS, where `/proc` does not exist: the pids
//! `/usr/sbin/lsof -b -w -nP -iTCP:<port> -sTCP:LISTEN -Fp` reports (one
//! `p<pid>` line per process, on any address; `-b` avoids calls that can
//! block on a hung file system). It costs a process spawn, so only the opt-in
//! attach to the user's own browser uses it, and it is killed after two
//! seconds.

#[cfg(target_os = "macos")]
const LSOF_LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

/// The pids holding a listener on `port`: empty when none does, `None` when
/// this platform cannot say (not macOS, or `lsof` did not finish in time).
pub fn listener_pids(port: u16) -> Option<Vec<u32>> {
    #[cfg(target_os = "macos")]
    {
        let text = run_lsof(&[
            "-b",
            "-w",
            "-nP",
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
            "-Fp",
        ])?;
        Some(parse_lsof_pids(&text))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = port;
        None
    }
}

/// `/usr/sbin/lsof` with `args`, killed after [`LSOF_LIMIT`]: its output,
/// empty when it found nothing (exit 1 with no output), `None` when it
/// failed or did not finish in time.
#[cfg(target_os = "macos")]
pub(crate) fn run_lsof(args: &[&str]) -> Option<String> {
    use std::io::Read;
    let mut child = std::process::Command::new("/usr/sbin/lsof")
        .args(args)
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).ok().map(|_| text)
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed() > LSOF_LIMIT {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let text = reader.join().ok()??;
    let ran = status.success() || (status.code() == Some(1) && text.is_empty());
    ran.then_some(text)
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
