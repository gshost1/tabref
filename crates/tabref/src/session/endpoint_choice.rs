use std::path::Path;
use std::time::Duration;

use tabref_cdp::{
    BrowserEndpoint, CdpConnection, EndpointSpec, LaunchOptions, connect, launch_chrome,
};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, RefStore};

use super::launch_record::{forget_launched, load_launched, save_launched};
use super::session_options::SessionOptions;

const DEFAULT_PORT: u16 = 9222;
const PROBE_BUDGET: Duration = Duration::from_millis(300);

/// Finds the browser and opens the one DevTools connection the command
/// uses. In order: the endpoint `--cdp`/`TABREF_CDP` names; the browser a
/// previous `open --launch` started; a browser on port 9222; and, when the
/// command allows it, a browser started now. A launch record is forgotten
/// only when its browser is gone: the connection is refused or its process
/// has exited. A launched browser that is slow to answer is busy, not gone,
/// so the error is returned rather than a second browser started.
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
    if let Some(found) = default_port(deadline) {
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

fn default_port(deadline: Deadline) -> Option<(BrowserEndpoint, CdpConnection)> {
    let probe = deadline.capped(PROBE_BUDGET);
    let endpoint = EndpointSpec::Port(DEFAULT_PORT).resolve(probe).ok()?;
    let connection = connect(&endpoint, deadline).ok()?;
    Some((endpoint, connection))
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
             --remote-debugging-port=9222 (or pass --cdp <port>)",
    )
    .with_details(serde_json::json!({ "kind": "cdp_no_browser" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "endpoint_choice_tests.rs"]
mod tests;
