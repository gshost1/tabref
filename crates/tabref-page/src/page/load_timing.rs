use serde_json::Value;
use tabref_core::{AppError, Deadline};

use super::evaluate::evaluate_quietly;
use crate::{CdpConnection, PageSession};

const TIMING: &str = r"(() => {
  const nav = performance.getEntriesByType('navigation')[0];
  if (!nav) return null;
  const dcl = nav.domContentLoadedEventEnd;
  const resources = performance.getEntriesByType('resource');
  return {
    commit: Math.round(nav.responseStart),
    response_end: Math.round(nav.responseEnd),
    dcl: Math.round(dcl),
    load: Math.round(nav.loadEventEnd),
    now: Math.round(performance.now()),
    requests: resources.length,
    after_dcl: resources.filter(entry => entry.startTime > dcl).length,
    open_requests: resources.filter(entry => entry.responseEnd === 0).length,
    last_response: Math.round(resources.reduce((last, entry) => Math.max(last, entry.responseEnd), 0)),
  };
})()";

/// The current document's own load timings, in milliseconds since its
/// navigation started, for a timing trace: when its response began and
/// ended, `DOMContentLoaded` and `load` ended (0 while not yet), the time
/// now, how many subresources it requested, how many of them started after
/// `DOMContentLoaded`, and when the last one answered. `None` for a page
/// without a navigation entry.
pub fn load_timing(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<Option<Value>, AppError> {
    let value = evaluate_quietly(connection, session, TIMING, deadline)?;
    Ok(value.is_object().then_some(value))
}
