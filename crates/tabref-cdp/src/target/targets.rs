use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use crate::transport::transport_error::transport_failed;
use crate::{CdpConnection, PageSession, PageTarget};

/// Lists the browser's page targets. Service workers, extensions and other
/// non-page targets are left out.
pub fn list_page_targets(
    connection: &mut CdpConnection,
    deadline: Deadline,
) -> Result<Vec<PageTarget>, AppError> {
    let result = connection.call("Target.getTargets", json!({}), None, deadline)?;
    let infos = result
        .get("targetInfos")
        .and_then(Value::as_array)
        .ok_or_else(|| transport_failed("Target.getTargets", "reply has no targetInfos"))?;
    Ok(infos
        .iter()
        .filter(|info| info.get("type").and_then(Value::as_str) == Some("page"))
        .filter_map(page_target)
        .collect())
}

/// Attaches a flattened session to `target_id` and releases the target if
/// it is paused waiting for a debugger, as targets reached through a
/// browser's remote-debugging server arrive.
pub fn attach_to_target(
    connection: &mut CdpConnection,
    target_id: &str,
    deadline: Deadline,
) -> Result<PageSession, AppError> {
    let result = connection.call(
        "Target.attachToTarget",
        json!({ "targetId": target_id, "flatten": true }),
        None,
        deadline,
    )?;
    let session_id = string_field(&result, "sessionId", "Target.attachToTarget")?;
    connection.call(
        "Runtime.runIfWaitingForDebugger",
        json!({}),
        Some(&session_id),
        deadline,
    )?;
    Ok(PageSession {
        target_id: target_id.to_owned(),
        session_id,
    })
}

/// Opens `url` in a new background tab and returns its target id. The tab
/// is never brought to the front.
pub fn create_background_tab(
    connection: &mut CdpConnection,
    url: &str,
    deadline: Deadline,
) -> Result<String, AppError> {
    let result = connection.call(
        "Target.createTarget",
        json!({ "url": url, "background": true }),
        None,
        deadline,
    )?;
    string_field(&result, "targetId", "Target.createTarget")
}

/// Brings a page target to the front of its window. In a headless browser
/// this only marks the page as the focused one.
pub fn activate_target(
    connection: &mut CdpConnection,
    target_id: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    connection
        .call(
            "Target.activateTarget",
            json!({ "targetId": target_id }),
            None,
            deadline,
        )
        .map(|_| ())
}

/// Closes a page target.
pub fn close_target(
    connection: &mut CdpConnection,
    target_id: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    connection
        .call(
            "Target.closeTarget",
            json!({ "targetId": target_id }),
            None,
            deadline,
        )
        .map(|_| ())
}

fn page_target(info: &Value) -> Option<PageTarget> {
    let text = |key: &str| info.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(PageTarget {
        target_id: text("targetId")?,
        url: text("url").unwrap_or_default(),
        title: text("title").unwrap_or_default(),
        attached: info
            .get("attached")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        browser_context_id: text("browserContextId"),
    })
}

fn string_field(result: &Value, key: &str, method: &str) -> Result<String, AppError> {
    result
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| transport_failed(method, format!("reply has no {key}")))
}

#[cfg(test)]
#[path = "targets_tests.rs"]
mod tests;
