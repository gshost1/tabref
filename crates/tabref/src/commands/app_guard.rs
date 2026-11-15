//! Apps that embed Chrome for their own interface (Electron: Slack, VS
//! Code, Obsidian), reached with `--cdp PORT`. Their window's page is the
//! app itself, so `open` never navigates it away; their web content is read
//! and acted on like any page.

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use crate::session::Session;

const EMBEDDED_RUNTIME_TOKEN: &str = " Electron/";
const WEB_SCHEMES: &[&str] = &["http:", "https:", "about:"];

/// Refuses to navigate the page at `current` when it is an app's own
/// interface: a page that is not on the web (`file://`, `app://`, a custom
/// scheme) in a browser whose user agent says it embeds Chrome. A web page,
/// a blank tab, or any page in a normal Chrome costs no extra call.
pub(crate) fn refuse_app_page(
    session: &mut Session,
    current: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    if is_web_page(current) {
        return Ok(());
    }
    let version = session
        .connection()
        .call("Browser.getVersion", json!({}), None, deadline)?;
    let agent = version
        .get("userAgent")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if is_embedded_runtime(agent) {
        return Err(not_a_browser(current));
    }
    Ok(())
}

/// Whether `user_agent` is that of an app embedding Chrome.
pub(crate) fn is_embedded_runtime(user_agent: &str) -> bool {
    user_agent.contains(EMBEDDED_RUNTIME_TOKEN)
}

/// Whether `url` is on the web or blank, which `open` may always replace.
pub(crate) fn is_web_page(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.is_empty() || WEB_SCHEMES.iter().any(|scheme| lower.starts_with(scheme))
}

fn not_a_browser(current: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!(
            "this tab is the app's own interface ({current}) in an app that embeds Chrome; open would replace it"
        ),
    )
    .with_suggestion(
        "Read and act on the app's page with snap, click and fill; open web addresses in a web browser",
    )
    .with_details(json!({ "kind": "cdp_not_a_browser", "url": current }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "app_guard_tests.rs"]
mod tests;
