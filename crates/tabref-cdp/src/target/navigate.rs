use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::frame_tree::list_frames;
use crate::transport::transport_error::{is_timeout, transport_failed};
use crate::{CdpConnection, CdpEvent, PageSession};

const LOAD_GRACE: std::time::Duration = std::time::Duration::from_millis(500);

/// Navigates the page and waits, bounded by `deadline`, for the new
/// document's load: the `load` lifecycle event carrying the frame and
/// loader `Page.navigate` returned, or 500 ms after its `DOMContentLoaded`
/// when `load` is later than that (ads and trackers hold `load` back for
/// seconds while the document itself is ready). A load event of any earlier document,
/// buffered or late, never satisfies the wait. A same-document navigation
/// returns at once.
pub fn navigate(
    connection: &mut CdpConnection,
    session: &PageSession,
    url: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    navigate_by(connection, session, url, deadline, deadline)
}

/// [`navigate`] with its two waits bounded apart. `commit` bounds the wait
/// for the browser to start the new document: `Page.navigate` answers only
/// once the response arrives, which a slow server, or a browser that has
/// just started and still loads its profile, holds back for seconds. `load`
/// bounds the wait for the load after that; a commit later than `load`
/// leaves the load wait until `commit`. A commit that misses `commit` is
/// `TIMEOUT` with kind `cdp_navigation_pending`.
pub fn navigate_by(
    connection: &mut CdpConnection,
    session: &PageSession,
    url: &str,
    commit: Deadline,
    load: Deadline,
) -> Result<(), AppError> {
    enable_load_events(connection, session, commit)?;
    let session_id = session.session_id.as_str();
    connection.discard_events(|event| is_any_load(event, session_id));
    let result = connection
        .call(
            "Page.navigate",
            json!({ "url": url }),
            Some(session_id),
            commit,
        )
        .map_err(|error| commit_timeout(error, url))?;
    let deadline = if load.is_expired() { commit } else { load };
    if let Some(error_text) = result
        .get("errorText")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
    {
        return Err(navigation_failed(url, error_text));
    }
    let Some(loader_id) = result.get("loaderId").and_then(Value::as_str) else {
        return Ok(());
    };
    let frame_id = result
        .get("frameId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    wait_for_document_load(connection, session_id, (frame_id, loader_id), deadline)
        .map_err(|error| load_timeout(error, url))
}

/// Waits, bounded by `deadline`, until the page's current document has
/// finished loading. A page that already finished returns at once. The
/// wait is bound to the main frame's loader read after the ready-state
/// check, so a buffered load event of an earlier document cannot end it.
pub fn wait_for_load(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<(), AppError> {
    enable_load_events(connection, session, deadline)?;
    let session_id = session.session_id.as_str();
    let state = connection.call(
        "Runtime.evaluate",
        json!({ "expression": "document.readyState", "returnByValue": true }),
        Some(session_id),
        deadline,
    )?;
    if state.pointer("/result/value").and_then(Value::as_str) == Some("complete") {
        return Ok(());
    }
    let main = list_frames(connection, session, deadline)?
        .into_iter()
        .next()
        .ok_or_else(|| transport_failed("Page.getFrameTree", "the page has no main frame"))?;
    let document = (main.frame_id.as_str(), main.loader_id.as_str());
    connection.discard_events(|event| {
        is_any_load(event, session_id) && !is_document_event(event, session_id, document, "load")
    });
    wait_for_document_load(connection, session_id, document, deadline)
        .map_err(|error| load_timeout(error, "the current document"))
}

fn wait_for_document_load(
    connection: &mut CdpConnection,
    session_id: &str,
    document: (&str, &str),
    deadline: Deadline,
) -> Result<(), AppError> {
    let first = connection.wait_for_event(
        |event| {
            is_document_event(event, session_id, document, "load")
                || is_document_event(event, session_id, document, "DOMContentLoaded")
        },
        deadline,
    )?;
    if is_document_event(&first, session_id, document, "load") {
        return Ok(());
    }
    match connection.wait_for_event(
        |event| is_document_event(event, session_id, document, "load"),
        deadline.capped(LOAD_GRACE),
    ) {
        Err(error) if is_timeout(&error) && !deadline.is_expired() => Ok(()),
        other => other.map(|_| ()),
    }
}

fn enable_load_events(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<(), AppError> {
    let session_id = Some(session.session_id.as_str());
    connection.call("Page.enable", json!({}), session_id, deadline)?;
    connection.call(
        "Page.setLifecycleEventsEnabled",
        json!({ "enabled": true }),
        session_id,
        deadline,
    )?;
    Ok(())
}

fn is_any_load(event: &CdpEvent, session_id: &str) -> bool {
    event.session_id.as_deref() == Some(session_id)
        && (event.method == "Page.loadEventFired"
            || (event.method == "Page.lifecycleEvent"
                && event.params.get("name").and_then(Value::as_str) == Some("load")))
}

fn is_document_event(
    event: &CdpEvent,
    session_id: &str,
    (frame_id, loader_id): (&str, &str),
    name: &str,
) -> bool {
    let field = |key: &str| event.params.get(key).and_then(Value::as_str);
    event.session_id.as_deref() == Some(session_id)
        && event.method == "Page.lifecycleEvent"
        && field("name") == Some(name)
        && field("frameId") == Some(frame_id)
        && field("loaderId") == Some(loader_id)
}

fn navigation_failed(url: &str, error_text: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("Navigation to {url} failed"),
    )
    .with_platform_detail(error_text)
    .with_details(json!({ "kind": "cdp_navigation_failed", "url": url }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

fn commit_timeout(error: AppError, url: &str) -> AppError {
    if !is_timeout(&error) {
        return error;
    }
    AppError::timeout(format!(
        "The browser did not start loading {url} before the deadline"
    ))
    .with_suggestion(
        "A slow server, or a browser that has just started and is still loading its \
             profile, holds a navigation back; repeat the same command, or pass a longer --timeout",
    )
    .with_details(json!({ "kind": "cdp_navigation_pending", "url": url, "retryable": true }))
    .with_disposition(DeliverySemantics::delivered_unverified())
}

fn load_timeout(error: AppError, what: &str) -> AppError {
    if !is_timeout(&error) {
        return error;
    }
    AppError::timeout(format!("{what} did not finish loading before the deadline"))
        .with_details(json!({ "kind": "cdp_load_timeout" }))
        .with_disposition(DeliverySemantics::delivered_unverified())
}

#[cfg(test)]
#[path = "navigate_tests.rs"]
mod tests;
