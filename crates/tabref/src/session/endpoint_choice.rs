use std::time::Duration;

use tabref_cdp::{
    BrowserEndpoint, CdpConnection, EndpointSpec, LaunchOptions, connect, launch_chrome,
};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode, RefStore};

use super::launch_record::{forget_launched, load_launched, save_launched};
use super::session_options::SessionOptions;

const DEFAULT_PORT: u16 = 9222;
const PROBE_BUDGET: Duration = Duration::from_millis(300);
const RECORD_BUDGET: Duration = Duration::from_millis(1_500);

/// Finds the browser and opens the one DevTools connection the command
/// uses. In order: the endpoint `--cdp`/`TABREF_CDP` names; the browser a
/// previous `open --launch` started; a browser on port 9222; and, when the
/// command allows it, a browser started now. A launch record whose browser
/// no longer answers is forgotten.
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
    if let Some(found) = launched_browser(store, deadline) {
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
) -> Option<(BrowserEndpoint, CdpConnection)> {
    let record = load_launched(store)?;
    let endpoint = record.endpoint().ok()?;
    match connect(&endpoint, deadline.capped(RECORD_BUDGET)) {
        Ok(connection) => Some((endpoint, connection)),
        Err(_) => {
            forget_launched(store);
            None
        }
    }
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
