use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tabref_core::{
    AppError, Deadline, DeliverySemantics, ErrorCode, create_new_private_dir, create_private_dir,
    new_snapshot_id,
};

use super::chrome_binary::find_chrome;
use super::launch_options::LaunchOptions;
use super::launched_browser::LaunchedBrowser;
use crate::read_active_port;

const POLL: Duration = Duration::from_millis(25);
/// Variables that mark the launching process (a tabref daemon) and must not
/// reach the browser or anything it starts.
const INHERITED_NOT: &[&str] = &[
    "TABREF_DAEMON",
    "TABREF_DAEMON_CDP",
    "TABREF_DAEMON_IDLE_MS",
];
const WINDOW_SIZE: &str = "--window-size=1280,900";

/// Starts a browser with a fresh throwaway profile and remote debugging on a
/// port the browser picks, and waits, bounded by `deadline`, until it writes
/// that port to the profile's `DevToolsActivePort`. The browser runs in its
/// own process group, so it outlives this process and the shell that ran it.
pub fn launch_chrome(
    options: &LaunchOptions,
    deadline: Deadline,
) -> Result<LaunchedBrowser, AppError> {
    let binary = match &options.binary {
        Some(binary) => binary.clone(),
        None => find_chrome()?,
    };
    let profile_dir = profile_dir(options)?;
    let mut command = Command::new(&binary);
    command
        .args(browser_args(options, &profile_dir))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for name in INHERITED_NOT {
        command.env_remove(name);
    }
    detach(&mut command);
    let mut child = command.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::ActionFailed,
            format!("Could not start {}", binary.display()),
        )
        .with_platform_detail(error.to_string())
        .with_details(serde_json::json!({ "kind": "chrome_spawn_failed" }))
        .with_disposition(DeliverySemantics::not_delivered())
    })?;
    let active = wait_for_port(&mut child, &profile_dir, deadline)?;
    let pid = child.id();
    std::thread::spawn(move || child.wait());
    Ok(LaunchedBrowser {
        pid,
        port: active.port,
        websocket_path: active.websocket_path,
        profile_dir,
        headless: !options.headed,
    })
}

pub(crate) fn browser_args(options: &LaunchOptions, profile_dir: &Path) -> Vec<String> {
    let mut args = vec![
        format!("--user-data-dir={}", profile_dir.display()),
        "--remote-debugging-port=0".to_owned(),
        "--no-first-run".to_owned(),
        "--no-default-browser-check".to_owned(),
        "--disable-search-engine-choice-screen".to_owned(),
        WINDOW_SIZE.to_owned(),
    ];
    if !options.headed {
        args.push("--headless=new".to_owned());
    }
    args.extend(options.extra_args.iter().cloned());
    args.push("about:blank".to_owned());
    args
}

fn profile_dir(options: &LaunchOptions) -> Result<PathBuf, AppError> {
    let root = options
        .profile_root
        .clone()
        .unwrap_or_else(std::env::temp_dir);
    create_private_dir(&root)?;
    let dir = root.join(format!("tabref-profile-{}", new_snapshot_id()));
    create_new_private_dir(&dir)?;
    Ok(dir)
}

fn wait_for_port(
    child: &mut Child,
    profile_dir: &Path,
    deadline: Deadline,
) -> Result<crate::ActivePort, AppError> {
    loop {
        if let Ok(active) = read_active_port(profile_dir) {
            return Ok(active);
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(AppError::new(
                ErrorCode::ActionFailed,
                format!("The browser exited before it opened DevTools ({status})"),
            )
            .with_suggestion("Run the browser binary by hand to see why it does not start")
            .with_details(serde_json::json!({ "kind": "chrome_exited" })));
        }
        if deadline.is_expired() {
            let _ = child.kill();
            return Err(
                AppError::timeout("The browser did not open DevTools before the deadline")
                    .with_details(serde_json::json!({ "kind": "chrome_launch_timeout" })),
            );
        }
        std::thread::sleep(POLL.min(deadline.remaining()));
    }
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn detach(_command: &mut Command) {}

#[cfg(test)]
#[path = "chrome_launch_tests.rs"]
mod tests;
