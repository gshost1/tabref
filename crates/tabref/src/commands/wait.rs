use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_cdp::{PageSession, is_timeout};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};
use tabref_page::{
    ElementState, ElementToken, LocateIntent, wait_for_state, wait_for_text, wait_for_url,
};

use super::act_target::resolve_target;
use super::download_lines::{download_json, download_line};
use super::download_wait::wait_download;
use super::target_spec::TargetSpec;
use super::wait_condition::Condition;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::find_matches::find_matches;

const QUIET: Duration = Duration::from_millis(500);
const MATCHES_SHOWN: usize = 5;

/// Waits, bounded by `--timeout`, until `condition` holds. A condition that
/// already holds returns at once; otherwise the wait ends on the page event
/// that decides it, never on a poll.
pub(crate) fn run(session: &mut Session, condition: Condition) -> Result<Outcome, AppError> {
    let started = Instant::now();
    let deadline = session.deadline();
    let (detail, extra) = wait_for(session, &condition, deadline, true)?;
    let elapsed = started.elapsed().as_millis();
    let mut text = format!("ok after {elapsed} ms waiting for {}", condition.describe());
    if !detail.is_empty() {
        text.push('\n');
        text.push_str(&detail);
    }
    let mut data = json!({ "elapsed_ms": elapsed, "detail": detail });
    if let (Some(object), Value::Object(more)) = (data.as_object_mut(), extra) {
        object.extend(more);
    }
    Ok(Outcome::new(text, data))
}

/// Waits for `condition` within `deadline`, and returns what to show
/// (with `show`, the lines that hold appearing text, with their refs; the
/// download a download wait returned) and any `--json` data to add.
pub(crate) fn wait_for(
    session: &mut Session,
    condition: &Condition,
    deadline: Deadline,
    show: bool,
) -> Result<(String, Value), AppError> {
    match condition {
        Condition::Text(text) | Condition::Gone(text) => {
            let page = session.page()?;
            let present = matches!(condition, Condition::Text(_));
            if !wait_for_text(session.connection(), &page, text, present, deadline)? {
                return Err(timed_out(condition, deadline));
            }
            if !present || !show {
                return Ok((String::new(), Value::Null));
            }
            let captured = session.capture()?;
            let lines = find_matches(&captured.tree, text, None);
            let shown = lines
                .into_iter()
                .take(MATCHES_SHOWN)
                .collect::<Vec<_>>()
                .join("\n");
            Ok((shown, Value::Null))
        }
        Condition::Url(part) => {
            let page = session.page()?;
            let reached = wait_for_url(session.connection(), &page, part, deadline)?
                .ok_or_else(|| timed_out(condition, deadline))?;
            Ok((reached, Value::Null))
        }
        Condition::Idle => {
            let page = session.page()?;
            wait_idle(session, &page, condition, deadline)?;
            Ok((String::new(), Value::Null))
        }
        Condition::Element { target, state, .. } => {
            let Some((page, token)) = element_target(session, target, state, deadline)? else {
                return Ok((String::new(), json!({ "now": "gone" })));
            };
            let waited = wait_for_state(session.connection(), &page, &token, state, deadline)?;
            if !waited.held {
                return Err(timed_out(condition, deadline).with_details(json!({
                    "kind": "wait_timeout",
                    "state": state.describe(),
                    "now": waited.now,
                })));
            }
            Ok((String::new(), json!({ "now": waited.now })))
        }
        Condition::Download => {
            let download = wait_download(session, deadline)?;
            Ok((
                download_line(&download),
                json!({ "download": download_json(&download) }),
            ))
        }
    }
}

fn wait_idle(
    session: &mut Session,
    page: &PageSession,
    condition: &Condition,
    deadline: Deadline,
) -> Result<(), AppError> {
    let connection = session.connection();
    connection.call(
        "Network.enable",
        json!({}),
        Some(&page.session_id),
        deadline,
    )?;
    let mut pending: HashSet<String> = HashSet::new();
    let mut quiet_since = Instant::now();
    loop {
        if pending.is_empty() && quiet_since.elapsed() >= QUIET {
            return Ok(());
        }
        if deadline.is_expired() {
            return Err(timed_out(condition, deadline));
        }
        let window = if pending.is_empty() {
            QUIET.saturating_sub(quiet_since.elapsed())
        } else {
            deadline.remaining()
        };
        let session_id = page.session_id.clone();
        let event = connection.wait_for_event(
            |event| {
                event.session_id.as_deref() == Some(&session_id)
                    && event.method.starts_with("Network.")
            },
            deadline.capped(window),
        );
        let event = match event {
            Ok(event) => event,
            Err(error) if is_timeout(&error) => continue,
            Err(error) => return Err(error),
        };
        let id = event
            .params
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        match event.method.as_str() {
            "Network.requestWillBeSent" => {
                pending.insert(id);
            }
            "Network.loadingFinished" | "Network.loadingFailed" => {
                if pending.remove(&id) && pending.is_empty() {
                    quiet_since = Instant::now();
                }
            }
            _ => {}
        }
    }
}

/// The element `target` names, for a wait on `state`: a ref even while it
/// is hidden, a name as the page shows it. `None` when it is not there (a
/// stale ref, a name the page does not show) and the state is `gone` or
/// `hidden`, which both mean no longer shown, so a name and a ref agree.
fn element_target(
    session: &mut Session,
    target: &TargetSpec,
    state: &ElementState,
    deadline: Deadline,
) -> Result<Option<(PageSession, ElementToken)>, AppError> {
    let absent_is_enough = state.absent();
    let found = match target {
        TargetSpec::Ref(text) => session
            .resolve_ref_or_hidden(text)
            .map(|(found, _)| (found.session, found.token)),
        TargetSpec::Find(_) => resolve_target(session, target, LocateIntent::Read, deadline)
            .map(|found| (found.session, found.token)),
    };
    match found {
        Ok(found) => Ok(Some(found)),
        Err(error)
            if absent_is_enough
                && matches!(error.code, ErrorCode::StaleRef | ErrorCode::ElementNotFound) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn timed_out(condition: &Condition, deadline: Deadline) -> AppError {
    AppError::timeout(format!(
        "waited {} ms for {}",
        deadline.elapsed().as_millis(),
        condition.describe()
    ))
    .with_details(json!({ "kind": "wait_timeout" }))
    .with_disposition(DeliverySemantics::not_delivered())
}
