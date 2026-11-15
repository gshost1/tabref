use std::path::Path;
use std::time::Duration;

use tabref_cdp::{
    BrowserEndpoint, CdpConnection, EndpointSpec, LaunchOptions, ListenerVerdict, ProcessView,
    check_listener, connect, is_chrome_family, launch_chrome,
};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, RefStore};

use super::launch_record::{forget_launched, load_launched, save_launched};
use super::session_options::SessionOptions;

const DEFAULT_PORT: u16 = 9222;
const PROBE_BUDGET: Duration = Duration::from_millis(300);

/// Finds the browser and opens the one DevTools connection the command
/// uses. tabref attaches only to a browser it can vouch for, in order: the
/// endpoint `--cdp`/`TABREF_CDP` names (the user chose it); the browser a
/// previous `open --launch` started, while the port is still held by a
/// process running that launch's profile; a Chrome of this user on port
/// 9222, checked to own the port before anything is sent to it (Linux only;
/// elsewhere the port must be passed with `--cdp`); and, when the command
/// allows it, a browser started now. Something else on 9222 is refused with
/// kind `cdp_endpoint_unverified`. A launch record is forgotten once its
/// browser is gone: the port is free, held by something else, refuses the
/// connection, or its process has exited. A launched browser that is slow
/// to answer is busy, not gone, so the error is returned rather than a
/// second browser started.
pub(crate) fn connect_browser(
    options: &SessionOptions,
    store: &RefStore,
    deadline: Deadline,
) -> Result<(BrowserEndpoint, CdpConnection), AppError> {
    if let Some(spec) = &options.cdp {
        let endpoint = EndpointSpec::parse(spec)?.resolve(deadline)?;
        let connection = connect(&endpoint, deadline)?;
        return Ok((endpoint, connection));
    }
    if let Some(found) = launched_browser(store, deadline)? {
        return Ok(found);
    }
    if let Some(found) = default_port(deadline, options.launch.is_some())? {
        return Ok(found);
    }
    match &options.launch {
        Some(launch) => start_browser(launch, store, deadline),
        None => Err(no_browser()),
    }
}

fn launched_browser(
    store: &RefStore,
    deadline: Deadline,
) -> Result<Option<(BrowserEndpoint, CdpConnection)>, AppError> {
    let Some(record) = load_launched(store) else {
        return Ok(None);
    };
    let Ok(endpoint) = record.endpoint() else {
        forget_launched(store);
        return Ok(None);
    };
    let ours = launched_from(&record.profile_dir);
    if matches!(
        check_listener(record.port, &ours),
        ListenerVerdict::Nothing | ListenerVerdict::Untrusted(_)
    ) {
        forget_launched(store);
        return Ok(None);
    }
    match connect(&endpoint, deadline) {
        Ok(connection) => Ok(Some((endpoint, connection))),
        Err(error) if browser_is_gone(&error, process_alive(record.pid)) => {
            forget_launched(store);
            Ok(None)
        }
        Err(error) if error.code == ErrorCode::Timeout => Err(error.with_suggestion(
            "The browser 'open --launch' started is busy; retry with a longer --timeout",
        )),
        Err(error) => Err(error),
    }
}

/// Whether a process runs the browser `open --launch` started with
/// `profile_dir`, as its command line names that profile.
pub(crate) fn launched_from(profile_dir: &Path) -> impl Fn(&ProcessView) -> bool + use<> {
    let flag = format!("--user-data-dir={}", profile_dir.display());
    move |process: &ProcessView| process.cmdline.contains(&flag)
}

/// Whether a failed connection to a launched browser means it is gone: the
/// port refused the connection, or its process is known to have exited.
pub(crate) fn browser_is_gone(error: &AppError, alive: Option<bool>) -> bool {
    error.kind() == Some("cdp_endpoint_unavailable") || alive == Some(false)
}

/// Whether process `pid` exists, where `/proc` can say; `None` elsewhere.
fn process_alive(pid: u32) -> Option<bool> {
    let proc_root = Path::new("/proc");
    proc_root
        .join("self")
        .exists()
        .then(|| proc_root.join(pid.to_string()).exists())
}

fn default_port(
    deadline: Deadline,
    can_launch: bool,
) -> Result<Option<(BrowserEndpoint, CdpConnection)>, AppError> {
    if !implicit_port(&check_listener(DEFAULT_PORT, &is_chrome_family), can_launch)? {
        return Ok(None);
    }
    let probe = deadline.capped(PROBE_BUDGET);
    let Ok(endpoint) = EndpointSpec::Port(DEFAULT_PORT).resolve(probe) else {
        return Ok(None);
    };
    Ok(connect(&endpoint, deadline)
        .ok()
        .map(|connection| (endpoint, connection)))
}

/// Whether port 9222 may be tried without being named: only when a Chrome
/// of this user holds it. Anything else listening there is refused, unless
/// the command may start a browser of its own instead.
pub(crate) fn implicit_port(verdict: &ListenerVerdict, can_launch: bool) -> Result<bool, AppError> {
    match verdict {
        ListenerVerdict::Trusted => Ok(true),
        ListenerVerdict::Nothing | ListenerVerdict::Unknown => Ok(false),
        ListenerVerdict::Untrusted(_) if can_launch => Ok(false),
        ListenerVerdict::Untrusted(reason) => Err(unverified_port(DEFAULT_PORT, reason)),
    }
}

fn unverified_port(port: u16, reason: &str) -> AppError {
    AppError::new(
        ErrorCode::PermDenied,
        format!("Port {port} is open, but not held by a Chrome of this user; tabref does not attach to it"),
    )
    .with_suggestion(format!(
        "Pass --cdp {port} if you started that browser yourself, or run 'tabref open <url> --launch' \
         for a browser of tabref's own"
    ))
    .with_platform_detail(reason)
    .with_details(serde_json::json!({ "kind": "cdp_endpoint_unverified", "port": port }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn start_browser(
    launch: &LaunchOptions,
    store: &RefStore,
    deadline: Deadline,
) -> Result<(BrowserEndpoint, CdpConnection), AppError> {
    let browser = launch_chrome(launch, deadline)?;
    save_launched(store, &browser)?;
    let endpoint = browser.endpoint()?;
    let connection = connect(&endpoint, deadline)?;
    Ok((endpoint, connection))
}

fn no_browser() -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        "No browser with DevTools is reachable",
    )
    .with_suggestion(
        "Run 'tabref open <url> --launch' to start one, or start Chrome with \
         --remote-debugging-port=<port> and pass --cdp <port>",
    )
    .with_details(serde_json::json!({ "kind": "cdp_no_browser" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "endpoint_choice_tests.rs"]
mod tests;
