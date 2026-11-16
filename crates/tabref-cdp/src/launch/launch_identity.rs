//! Which run of a browser a DevTools endpoint is. Each time Chrome opens its
//! DevTools server it names it with a fresh id, the browser WebSocket path
//! `/devtools/browser/<id>`: line two of the profile's `DevToolsActivePort`,
//! and the `webSocketDebuggerUrl` that `/json/version` advertises. A pid can
//! be reused and a port taken by another browser, but no later run has the
//! id an earlier one had, so the id says whether a port is still held by the
//! browser `open --launch` started.

use tabref_core::{AppError, DeliverySemantics, ErrorCode};

use crate::{ActivePort, LaunchedBrowser};

const BROWSER_PATH: &str = "/devtools/browser/";

/// The id in a browser WebSocket path `/devtools/browser/<id>`; `None` for
/// any other path.
pub fn browser_guid(websocket_path: &str) -> Option<&str> {
    websocket_path
        .strip_prefix(BROWSER_PATH)
        .filter(|id| !id.is_empty() && !id.contains(['/', '?', '#']))
}

/// Whether `websocket_path` names the browser run `record` recorded.
pub fn same_browser_run(record: &LaunchedBrowser, websocket_path: &str) -> bool {
    browser_guid(websocket_path).is_some_and(|id| browser_guid(&record.websocket_path) == Some(id))
}

/// `Ok` when the record's profile `DevToolsActivePort`, as `read`, still
/// names the record's port and browser run. A file that is missing, cannot
/// be read, or names another port or run is kind `cdp_launch_unverified`:
/// tabref then cannot say the port is still that browser's.
pub fn check_active_port(
    record: &LaunchedBrowser,
    read: Result<ActivePort, AppError>,
) -> Result<(), AppError> {
    let file = read.map_err(|error| unverified(record, "could not be read", &error.message))?;
    if file.port != record.port {
        return Err(unverified(
            record,
            &format!("names port {}", file.port),
            "the browser on that profile is not the one recorded",
        ));
    }
    if !same_browser_run(record, &file.websocket_path) {
        return Err(unverified(
            record,
            "names another browser run",
            "its DevTools id differs from the one recorded at launch",
        ));
    }
    Ok(())
}

fn unverified(record: &LaunchedBrowser, what: &str, detail: &str) -> AppError {
    let path = record.profile_dir.join("DevToolsActivePort");
    AppError::new(
        ErrorCode::PermDenied,
        format!(
            "The browser 'open --launch' started (pid {}) cannot be confirmed on port {}: its \
             DevToolsActivePort at {} {what}; tabref does not attach to it",
            record.pid,
            record.port,
            path.display()
        ),
    )
    .with_suggestion(format!(
        "Quit that browser (pid {}), then run 'tabref open <url> --launch' for a new one",
        record.pid
    ))
    .with_platform_detail(detail)
    .with_details(serde_json::json!({
        "kind": "cdp_launch_unverified",
        "port": record.port,
        "path": path.display().to_string(),
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "launch_identity_tests.rs"]
mod tests;
