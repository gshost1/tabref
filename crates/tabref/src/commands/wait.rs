use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_cdp::{PageSession, is_timeout};
use tabref_core::AppError;
use tabref_page::{PageInfo, snapshot_page_within_budget};

use super::wait_condition::Condition;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::find_matches::find_matches;

const POLL: Duration = Duration::from_millis(100);
const QUIET: Duration = Duration::from_millis(500);

/// Waits, bounded by `--timeout`, until `condition` holds, checking as soon
/// as it starts, so a condition that already holds returns at once.
pub(crate) fn run(session: &mut Session, condition: Condition) -> Result<Outcome, AppError> {
    let started = Instant::now();
    let page = session.page()?;
    let detail = match &condition {
        Condition::Text(text) => poll(session, &condition, |session| {
            text_matches(session, &page, text, true)
        })?,
        Condition::Gone(text) => poll(session, &condition, |session| {
            text_matches(session, &page, text, false)
        })?,
        Condition::Url(url) => poll(session, &condition, |session| {
            let deadline = session.deadline();
            let info = PageInfo::read(session.connection(), &page.target_id, deadline)?;
            Ok(info.url.contains(url.as_str()).then_some(info.url))
        })?,
        Condition::Idle => {
            wait_idle(session, &page, &condition)?;
            "network quiet".to_owned()
        }
    };
    let elapsed = started.elapsed().as_millis();
    let mut text = format!("ok after {elapsed} ms waiting for {}", condition.describe());
    if !detail.is_empty() && !matches!(condition, Condition::Idle) {
        text.push('\n');
        text.push_str(&detail);
    }
    Ok(Outcome::new(
        text,
        json!({ "elapsed_ms": elapsed, "detail": detail }),
    ))
}

fn poll(
    session: &mut Session,
    condition: &Condition,
    mut check: impl FnMut(&mut Session) -> Result<Option<String>, AppError>,
) -> Result<String, AppError> {
    loop {
        if let Some(detail) = check(session)? {
            return Ok(detail);
        }
        let deadline = session.deadline();
        if deadline.remaining() <= POLL {
            return Err(timed_out(condition, deadline.timeout_ms()));
        }
        std::thread::sleep(POLL);
    }
}

fn text_matches(
    session: &mut Session,
    page: &PageSession,
    text: &str,
    want_present: bool,
) -> Result<Option<String>, AppError> {
    let deadline = session.deadline();
    let snapshot = snapshot_page_within_budget(session.connection(), page, deadline)?;
    let present = !find_matches(&snapshot.tree, text, None).is_empty();
    if present != want_present {
        return Ok(None);
    }
    if !want_present {
        return Ok(Some(String::new()));
    }
    let captured = session.capture()?;
    Ok(Some(find_matches(&captured.tree, text, None).join("\n")))
}

fn wait_idle(
    session: &mut Session,
    page: &PageSession,
    condition: &Condition,
) -> Result<(), AppError> {
    let deadline = session.deadline();
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
            return Err(timed_out(condition, deadline.timeout_ms()));
        }
        let window = if pending.is_empty() {
            QUIET.saturating_sub(quiet_since.elapsed())
        } else {
            POLL
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

fn timed_out(condition: &Condition, timeout_ms: u64) -> AppError {
    AppError::timeout(format!(
        "Waited {timeout_ms} ms for {}",
        condition.describe()
    ))
    .with_details(json!({ "kind": "wait_timeout" }))
    .with_disposition(tabref_core::DeliverySemantics::not_delivered())
}
