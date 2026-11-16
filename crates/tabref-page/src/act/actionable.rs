use std::sync::LazyLock;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::action_need::ActionNeed;
use super::blocker::Blocker;
use super::drawn_element::CONTROLS;
use super::element_call::ElementCall;
use super::element_errors::stale;
use super::gate_script::GATE;
use super::remote_element::{call_function, with_element};
use super::unactionable::Unactionable;
use crate::{CdpConnection, ElementToken, PageSession};

const LONGEST_WAIT: Duration = Duration::from_millis(1500);
const POLL: Duration = Duration::from_millis(50);

static CHECK: LazyLock<String> = LazyLock::new(|| {
    format!("function(need, controls) {{ return ({GATE})(this, need, controls); }}")
});

/// Checks that the node can take an action that needs `need`: attached,
/// shown (rendered, not `visibility: hidden`, not inside an inert or
/// `aria-hidden` section; a `display: contents` node by what its children
/// draw), with a box when the action presses it, enabled unless it only
/// hovers or shows, and not read-only for a fill. A node that is not ready
/// is checked again every 50 ms for up to 1.5 s, at most half of what is
/// left of `deadline`, on one resolved object. A node with no size does not
/// grow one by waiting, so it is reported at once, as a detached node is
/// `STALE_REF` at once. Returns `None` when the node is ready, or why it was
/// not.
pub fn wait_actionable(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    need: ActionNeed,
    deadline: Deadline,
) -> Result<Option<Unactionable>, AppError> {
    let started = Instant::now();
    let budget = LONGEST_WAIT.min(deadline.remaining() / 2);
    with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            let mut slept = false;
            loop {
                let state = call_function(connection, session, object, check(need), deadline)?;
                let Some(wire) = state.get("blocker").and_then(Value::as_str) else {
                    return Ok(None);
                };
                if wire == "detached" {
                    return Err(stale(token, "the node is no longer in its document"));
                }
                let Some(blocker) = Blocker::from_wire(wire) else {
                    return Ok(None);
                };
                let waited = started.elapsed();
                if blocker.is_settled() || waited + POLL >= budget {
                    let detail = state.get("detail").and_then(Value::as_str);
                    return Ok(Some(Unactionable {
                        blocker,
                        detail: detail.unwrap_or(wire).to_owned(),
                        waited_ms: if slept {
                            u64::try_from(waited.as_millis()).unwrap_or(u64::MAX)
                        } else {
                            0
                        },
                    }));
                }
                std::thread::sleep(POLL);
                slept = true;
            }
        },
    )
}

fn check(need: ActionNeed) -> ElementCall {
    ElementCall {
        function: CHECK.as_str(),
        arguments: vec![need.spec(), json!(CONTROLS)],
    }
}

#[cfg(test)]
#[path = "actionable_tests.rs"]
mod tests;
