//! Apps that embed Chrome for their own interface (Electron: Slack, VS
//! Code, Obsidian), reached with `--cdp PORT`. Every window of such an app
//! is the app itself, whether it shows a local file or an https page, so
//! `open` never navigates one away or opens a new one; the app's web
//! content is read and acted on like any page.

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use crate::session::Session;

const EMBEDDED_RUNTIME_TOKEN: &str = " Electron/";

/// Refuses `open`, with or without `--new-tab`, in a browser whose user
/// agent says it is an app embedding Chrome. Costs one
/// `Browser.getVersion`.
pub(crate) fn refuse_in_app(session: &mut Session, deadline: Deadline) -> Result<(), AppError> {
    let version = session
        .connection()
        .call("Browser.getVersion", json!({}), None, deadline)?;
    let agent = version
        .get("userAgent")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if is_embedded_runtime(agent) {
        return Err(not_a_browser());
    }
    Ok(())
}

/// Whether `user_agent` is that of an app embedding Chrome.
pub(crate) fn is_embedded_runtime(user_agent: &str) -> bool {
    user_agent.contains(EMBEDDED_RUNTIME_TOKEN)
}

fn not_a_browser() -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        "this is an app that embeds Chrome, and its windows are the app itself; open would replace the app's page or open a new app window",
    )
    .with_suggestion(
        "Read and act on the app's page with snap, click, fill and js; open web addresses in a web browser",
    )
    .with_details(json!({ "kind": "cdp_not_a_browser" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "app_guard_tests.rs"]
mod tests;
