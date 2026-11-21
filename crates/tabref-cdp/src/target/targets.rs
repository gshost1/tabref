use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

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
    attach_to_target_with(connection, target_id, &[], deadline)
}

/// Why a tab that attached may not answer the first requests on its
/// session: a JavaScript dialog that opened while no tabref connection was
/// attached blocks the page, and a connection made after it can neither see
/// nor answer it.
const BLOCKED_BY_DIALOG: &str = "a dialog may be open or the page is busy; answer any dialog in \
    the browser window, or wait for the page to finish, then retry. The daemon can catch future dialogs.";

/// [`attach_to_target`], also sending `follow_ups` on the new session in the
/// same round trip as the release, so a browser that is slow to answer (one
/// that has just started) is waited on once instead of once per call.
pub fn attach_to_target_with(
    connection: &mut CdpConnection,
    target_id: &str,
    follow_ups: &[(&str, Value)],
    deadline: Deadline,
) -> Result<PageSession, AppError> {
    let result = connection.call(
        "Target.attachToTarget",
        json!({ "targetId": target_id, "flatten": true }),
        None,
        deadline,
    )?;
    let session_id = string_field(&result, "sessionId", "Target.attachToTarget")?;
    connection.note_session_tab(&session_id, target_id);
    let mut calls = vec![("Runtime.runIfWaitingForDebugger", json!({}))];
    calls.extend(follow_ups.iter().cloned());
    attach_calls(connection, target_id, &session_id, &calls, deadline).map_err(|error| {
        if error.code == ErrorCode::Timeout {
            error.with_suggestion(BLOCKED_BY_DIALOG)
        } else {
            error
        }
    })?;
    Ok(PageSession {
        target_id: target_id.to_owned(),
        session_id,
    })
}

fn attach_calls(
    connection: &mut CdpConnection,
    target_id: &str,
    session_id: &str,
    calls: &[(&str, Value)],
    deadline: Deadline,
) -> Result<(), AppError> {
    let started = Instant::now();
    let mut pending = Vec::with_capacity(calls.len());
    let outcome = (|| {
        for (method, params) in calls {
            pending.push(connection.send(method, params.clone(), Some(session_id), deadline)?);
        }
        let first = if connection.probes_attach(target_id)
            && calls.iter().any(|(method, _)| *method == "Page.enable")
        {
            deadline.capped(Duration::from_millis(
                (deadline.timeout_ms() / 4).clamp(1, 2000),
            ))
        } else {
            deadline
        };
        let mut progressed = false;
        match attach_replies(connection, &mut pending, first, &mut progressed) {
            Err(error)
                if error.code == ErrorCode::Timeout
                    && first != deadline
                    && !deadline.is_expired() =>
            {
                if !progressed
                    && connection
                        .call(
                            "Target.getTargetInfo",
                            json!({ "targetId": target_id }),
                            None,
                            deadline.capped(Duration::from_millis(35)),
                        )
                        .is_ok()
                {
                    match attach_replies(
                        connection,
                        &mut pending,
                        deadline.capped(Duration::from_millis(70)),
                        &mut progressed,
                    ) {
                        Err(error)
                            if error.code == ErrorCode::Timeout && !deadline.is_expired() =>
                        {
                            if !progressed {
                                let waited_ms = started.elapsed().as_millis();
                                return Err(AppError::new(ErrorCode::ActionFailed,
                                            format!("Waited {waited_ms} ms for page initialization; a dialog may be open or the page is busy"))
                                            .with_details(json!({"kind":"cdp_page_unresponsive","waited_ms":waited_ms}))
                                            .with_disposition(DeliverySemantics::not_delivered())
                                            .with_suggestion(BLOCKED_BY_DIALOG));
                            }
                        }
                        outcome => return outcome,
                    }
                }
                attach_replies(connection, &mut pending, deadline, &mut progressed)
            }
            outcome => outcome,
        }
    })();
    for id in pending {
        connection.abandon(id);
    }
    outcome
}

fn attach_replies(
    connection: &mut CdpConnection,
    pending: &mut Vec<u64>,
    deadline: Deadline,
    progressed: &mut bool,
) -> Result<(), AppError> {
    while !pending.is_empty() {
        let (id, outcome) = connection.next_reply(pending, deadline)?;
        pending.retain(|waiting| *waiting != id);
        outcome?;
        *progressed = true;
    }
    Ok(())
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
    let target_id = string_field(&result, "targetId", "Target.createTarget")?;
    connection.note_fresh_target(&target_id);
    Ok(target_id)
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

#[cfg(test)]
#[path = "targets_deadline_tests.rs"]
mod deadline_tests;

#[cfg(test)]
#[path = "targets_fresh_tests.rs"]
mod fresh_tests;
