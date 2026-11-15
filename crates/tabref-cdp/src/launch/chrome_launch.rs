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
use super::process_start::process_start;
use crate::{
    ActivePort, ListenerVerdict, ProcessView, check_listener, is_chrome_family, read_active_port,
};

const POLL: Duration = Duration::from_millis(25);
/// Variables that mark the launching process (a tabref daemon) and must not
/// reach the browser or anything it starts.
const INHERITED_NOT: &[&str] = &[
    "TABREF_DAEMON",
    "TABREF_DAEMON_CDP",
    "TABREF_DAEMON_IDLE_MS",
];
const WINDOW_SIZE: &str = "--window-size=1280,900";

/// Starts a browser with remote debugging on a port the browser picks, and
/// waits, bounded by `deadline`, until it writes that port to the profile's
/// `DevToolsActivePort`. The profile is a fresh throwaway one, or the
/// persistent `options.profile_dir` (created private to the user when new),
/// whose `DevToolsActivePort` from an earlier run is not taken for this
/// one's. The browser runs in its own process group, so it outlives this
/// process and the shell that ran it.
pub fn launch_chrome(
    options: &LaunchOptions,
    deadline: Deadline,
) -> Result<LaunchedBrowser, AppError> {
    let binary = match &options.binary {
        Some(binary) => binary.clone(),
        None => find_chrome()?,
    };
    let profile_dir = profile_dir(options)?;
    let before = port_file_print(&profile_dir);
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
    let active = wait_for_port(&mut child, &profile_dir, before.as_deref(), deadline)?;
    let pid = child.id();
    let started = process_start(pid);
    std::thread::spawn(move || child.wait());
    Ok(LaunchedBrowser {
        pid,
        port: active.port,
        websocket_path: active.websocket_path,
        profile_dir,
        headless: !options.headed,
        started,
        persistent: options.profile_dir.is_some(),
    })
}

/// What the profile's `DevToolsActivePort` is now (contents, size, time,
/// and on Unix its inode), so a launch can tell the file its browser writes
/// from one an earlier browser on the profile left; `None` without a file.
pub(crate) fn port_file_print(profile_dir: &Path) -> Option<String> {
    let path = profile_dir.join("DevToolsActivePort");
    let metadata = std::fs::metadata(&path).ok()?;
    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&metadata);
    #[cfg(not(unix))]
    let inode = 0_u64;
    Some(format!(
        "{inode}:{}:{:?}:{contents}",
        metadata.len(),
        metadata.modified().ok()
    ))
}

/// The profile's `DevToolsActivePort`, once it differs from `before` (what
/// it was before the launch), so a file an earlier browser left is never
/// taken for this one's.
pub(crate) fn changed_port_file(profile_dir: &Path, before: Option<&str>) -> Option<ActivePort> {
    let now = port_file_print(profile_dir)?;
    if before == Some(now.as_str()) {
        return None;
    }
    read_active_port(profile_dir).ok()
}

/// Whether the browser just started holds `port`: the child itself, or (a
/// binary that hands off to another process) a Chrome-family process running
/// this profile. Where the owner cannot be checked, the changed port file is
/// taken as it is.
fn launched_holds(port: u16, child: u32, profile_dir: &Path) -> bool {
    let flag = format!("--user-data-dir={}", profile_dir.display());
    let ours = |process: &ProcessView| {
        process.pid == child || (is_chrome_family(process) && process.args().contains(&flag))
    };
    matches!(
        check_listener(port, &ours),
        ListenerVerdict::Trusted(_) | ListenerVerdict::Unknown
    )
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
    if let Some(dir) = &options.profile_dir {
        if !dir.is_dir() {
            create_private_dir(dir)?;
        }
        return Ok(dir.clone());
    }
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
    before: Option<&str>,
    deadline: Deadline,
) -> Result<ActivePort, AppError> {
    loop {
        let exited = child.try_wait();
        if let (Ok(None), Some(active)) = (&exited, changed_port_file(profile_dir, before)) {
            if launched_holds(active.port, child.id(), profile_dir) {
                return Ok(active);
            }
        }
        if let Ok(Some(status)) = exited {
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
