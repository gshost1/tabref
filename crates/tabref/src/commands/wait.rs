use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_cdp::{PageSession, is_timeout};
use tabref_core::{AppError, Deadline, DeliverySemantics};
use tabref_page::{wait_for_text, wait_for_url};

use super::downloads::{download_json, download_line, wait_download};
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
    if condition == Condition::Download {
        let download = wait_download(session, deadline)?;
        let elapsed = started.elapsed().as_millis();
        let line = download_line(&download);
        return Ok(Outcome::new(
            format!(
                "ok after {elapsed} ms waiting for {}\n{line}",
                condition.describe()
            ),
            json!({ "elapsed_ms": elapsed, "detail": line, "download": download_json(&download) }),
        ));
    }
    let detail = wait_for(session, &condition, deadline, true)?;
    let elapsed = started.elapsed().as_millis();
    let mut text = format!("ok after {elapsed} ms waiting for {}", condition.describe());
    if !detail.is_empty() {
        text.push('\n');
        text.push_str(&detail);
    }
    Ok(Outcome::new(
        text,
        json!({ "elapsed_ms": elapsed, "detail": detail }),
    ))
}

/// Waits for `condition` within `deadline`. With `show`, appearing text is
/// reported as the lines that hold it, with their refs.
pub(crate) fn wait_for(
    session: &mut Session,
    condition: &Condition,
    deadline: Deadline,
    show: bool,
) -> Result<String, AppError> {
    let page = match condition {
        Condition::Download => {
            return wait_download(session, deadline).map(|download| download_line(&download));
        }
        _ => session.page()?,
    };
    match condition {
        Condition::Text(text) | Condition::Gone(text) => {
            let present = matches!(condition, Condition::Text(_));
            if !wait_for_text(session.connection(), &page, text, present, deadline)? {
                return Err(timed_out(condition, deadline));
            }
            if !present || !show {
                return Ok(String::new());
            }
            let captured = session.capture()?;
            let lines = find_matches(&captured.tree, text, None);
            Ok(lines
                .into_iter()
                .take(MATCHES_SHOWN)
                .collect::<Vec<_>>()
                .join("\n"))
        }
        Condition::Url(part) => wait_for_url(session.connection(), &page, part, deadline)?
            .ok_or_else(|| timed_out(condition, deadline)),
        Condition::Idle => {
            wait_idle(session, &page, condition, deadline)?;
            Ok(String::new())
        }
        Condition::Download => {
            wait_download(session, deadline).map(|download| download_line(&download))
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

fn timed_out(condition: &Condition, deadline: Deadline) -> AppError {
    AppError::timeout(format!(
        "waited {} ms for {}",
        deadline.elapsed().as_millis(),
        condition.describe()
    ))
    .with_details(json!({ "kind": "wait_timeout" }))
    .with_disposition(DeliverySemantics::not_delivered())
}
