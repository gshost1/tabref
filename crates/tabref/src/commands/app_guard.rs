//! Apps that embed Chrome for their own interface (Electron: Slack, VS
//! Code, Obsidian), reached with `--cdp PORT`. Every window of such an app
//! is the app itself, whether it shows a local file or an https page, so
//! `open` never navigates one away or opens a new one; the app's web
//! content is read and acted on like any page.

use serde_json::{Value, json};
use tabref_cdp::EndpointSource;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use crate::session::Session;

const EMBEDDED_RUNTIME_TOKEN: &str = " Electron/";

/// Refuses `open`, with or without `--new-tab`, in a browser whose user
/// agent says it is an app embedding Chrome. A browser `open --launch`
/// started is Chrome and is not asked; for one found through
/// `/json/version` the user agent it gave is used; otherwise the browser is
/// asked with `Browser.getVersion`. The page's own user-agent override never
/// counts, as these read the browser's. Fails closed: when the browser
/// cannot say what it is, the error stands and nothing is opened.
pub(crate) fn refuse_in_app(session: &mut Session, deadline: Deadline) -> Result<(), AppError> {
    let endpoint = session.endpoint().clone();
    if endpoint.source == EndpointSource::Launched {
        return Ok(());
    }
    let agent = match endpoint.user_agent {
        Some(agent) => agent,
        None => {
            let version =
                session
                    .connection()
                    .call("Browser.getVersion", json!({}), None, deadline)?;
            version
                .get("userAgent")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        }
    };
    match embedded_runtime(&agent) {
        Some(runtime) => Err(not_a_browser(runtime)),
        None => Ok(()),
    }
}

/// The runtime token (`Electron/32.1.0`) when `user_agent` is that of an
/// app embedding Chrome.
pub(crate) fn embedded_runtime(user_agent: &str) -> Option<&str> {
    let at = user_agent.find(EMBEDDED_RUNTIME_TOKEN)?;
    let token = &user_agent[at + 1..];
    Some(token.split_whitespace().next().unwrap_or(token))
}

fn not_a_browser(runtime: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        "this is an app that embeds Chrome, and its windows are the app itself; open would replace the app's page or open a new app window",
    )
    .with_suggestion(
        "Read and act on the app's page with snap, click and fill (js to read it); open web addresses in a web browser",
    )
    .with_details(json!({ "kind": "cdp_not_a_browser", "runtime": runtime }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "app_guard_tests.rs"]
mod tests;
