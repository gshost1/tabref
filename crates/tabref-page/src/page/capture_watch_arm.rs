use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::{ARM, CaptureWatch, evaluate_quietly, opaque, script, trace};
use crate::{CdpConnection, PageSession};

impl CaptureWatch {
    pub fn arm_with_budget(
        connection: &mut CdpConnection,
        session: &PageSession,
        token: &str,
        deadline: Deadline,
        initial_budget: Duration,
    ) -> Result<Self, AppError> {
        arm(
            connection,
            session,
            token,
            deadline.capped(initial_budget),
            deadline,
        )
    }
}

/// Arms a watch named `token` in the page, replacing any earlier one.
pub fn arm_capture_watch(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &str,
    deadline: Deadline,
) -> Result<CaptureWatch, AppError> {
    arm(connection, session, token, deadline, deadline)
}

fn arm(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &str,
    initial: Deadline,
    deadline: Deadline,
) -> Result<CaptureWatch, AppError> {
    let mut reply = evaluate_quietly(connection, session, &script(ARM, token), initial)
        .inspect_err(|error| {
            trace::record(
                "arm.error",
                || json!({"token":token,"step":"initial","error":error.to_string()}),
            );
        })?;
    trace::record("arm.scan", || json!({"token":token,"reply":reply}));
    if let Some(urls) = reply.get("opaque").and_then(Value::as_array) {
        let deadline = deadline.capped(Duration::from_millis(500));
        match opaque::prepare(connection, session, token, urls, deadline) {
            Ok(()) => {
                let rearm = ARM.replace("if (document.visibilityState === 'hidden')", "if (true)");
                reply = evaluate_quietly(connection, session, &script(&rearm, token), deadline)
                    .inspect_err(|error| {
                        trace::record(
                            "arm.error",
                            || json!({"token":token,"step":"approved","error":error.to_string()}),
                        );
                    })?;
                trace::record("arm.approved", || json!({"token":token,"reply":reply}));
            }
            Err(error) => trace::record(
                "opaque.error",
                || json!({"token":token,"error":error.to_string()}),
            ),
        }
    }
    Ok(CaptureWatch {
        token: token.to_owned(),
        blind: reply.get("blind").and_then(Value::as_bool).unwrap_or(true),
    })
}
