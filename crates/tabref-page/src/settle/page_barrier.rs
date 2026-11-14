use serde_json::json;
use tabref_core::{AppError, Deadline};

use crate::{CdpConnection, PageSession};

/// One turn of the page's event loop, posted through a message channel
/// rather than a timer, since a background tab throttles timers to once a
/// second. Tasks an action queued, such as a form submission, run first.
const ONE_TASK: &str = "new Promise(resolve => { const c = new MessageChannel(); c.port1.onmessage = () => resolve(0); c.port2.postMessage(0); })";

/// Waits for one turn of the page's event loop. Whether a navigation began
/// shows in the events that arrive before the reply. An error says the
/// page's context went away under the evaluation, which a navigation does.
pub(crate) fn page_turn(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<(), AppError> {
    let reply = connection.call(
        "Runtime.evaluate",
        json!({ "expression": ONE_TASK, "awaitPromise": true, "returnByValue": true }),
        Some(&session.session_id),
        deadline,
    )?;
    if reply.get("exceptionDetails").is_some() {
        return Err(AppError::internal("the page's context changed"));
    }
    Ok(())
}
