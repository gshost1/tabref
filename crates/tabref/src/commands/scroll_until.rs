use std::time::Duration;

use serde_json::json;
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline};
use tabref_page::{LocateIntent, ScrollOutcome, ScrollSite};

use super::{PAGE_STEP, report, step_outcome};
use crate::commands::act_target::{ActTarget, resolve_target};
use crate::commands::{find, scroll, target_spec::TargetSpec};
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

const MAX_STEPS: u64 = 60;
const MAX_TIME: Duration = Duration::from_secs(8);

pub(crate) fn run_loop(
    session: &mut Session,
    direction: &str,
    target: Option<&str>,
    px: Option<u32>,
    (until, to_end): (Option<&[String]>, bool),
) -> Result<Outcome, AppError> {
    let until = match until {
        Some([text]) => Some(text.as_str()),
        Some(words) => return Err(super::scroll_batch::quote_until_value(&words.join(" "))),
        None => None,
    };
    if until.is_none() && !to_end {
        return scroll::run(session, direction, target, px);
    }
    let container = target.map(TargetSpec::from_arg).transpose()?;
    let deadline = session.deadline();
    scroll_loop(
        session,
        direction,
        container.as_ref(),
        px,
        (until, to_end, deadline),
    )
}

pub(super) fn scroll_loop(
    session: &mut Session,
    direction: &str,
    container: Option<&TargetSpec>,
    px: Option<u32>,
    (until, to_end, deadline): (Option<&str>, bool, Deadline),
) -> Result<Outcome, AppError> {
    if until.is_none() && !to_end {
        return scroll::scroll(session, direction, container, px, deadline);
    }
    let direction = direction.trim().to_ascii_lowercase();
    let step = f64::from(px.unwrap_or(PAGE_STEP));
    let delta = match direction.as_str() {
        "down" => (0.0, step),
        "up" => (0.0, -step),
        "right" => (step, 0.0),
        "left" => (-step, 0.0),
        _ => {
            return Err(AppError::invalid_args(
                "--until and --to-end require a scroll direction, not a REF",
            ));
        }
    };
    let deadline = deadline.capped(MAX_TIME);
    let container = container
        .map(|spec| resolve_target(session, spec, LocateIntent::Click, deadline))
        .transpose()?;
    let mut total = ScrollOutcome::default();
    let mut steps = 0;
    let mut found = probe_until(session, container.as_ref(), until, deadline)?;
    let mut cap = None;
    while !found {
        if let Some(reason) = cap_reason(steps, deadline.is_expired()) {
            cap = Some(reason);
            break;
        }
        let next = match step_outcome(session, container.as_ref(), delta, deadline) {
            Ok(next) => next,
            Err(error) if is_timeout(&error) && deadline.is_expired() => {
                cap = Some("time cap (8 s or session deadline)");
                break;
            }
            Err(error) => return Err(error),
        };
        steps += 1;
        let ended = stopped(&next);
        accumulate(&mut total, next);
        if !deadline.is_expired() {
            found = probe_until(session, container.as_ref(), until, deadline)?;
        }
        if ended {
            break;
        }
    }
    let reason = cap.unwrap_or(match direction.as_str() {
        "up" => "the top",
        "left" => "the left end",
        "right" => "the right end",
        _ => "the bottom",
    });
    let mut output = report(&direction, step, &total);
    output.data["steps"] = json!(steps);
    output.data["until"] = json!(until);
    output.data["found"] = json!(found);
    output.data["cap"] = json!(cap);
    output.data["total_px"] = json!(step * steps as f64);
    output.data["first_item"] = json!(total.first);
    output.data["last_item"] = json!(total.last);
    output.text = match until {
        Some(text) if found => found_head(text, steps, true),
        Some(text) => not_found_head(text, steps, reason, cap.is_some()),
        None => end_summary(&direction, step, steps, &total, (reason, cap.is_some())),
    };
    if found && let Some(text) = until {
        let matches = find::search(session, text, None, 20, false)?;
        output.text = found_head(text, steps, !matches.lines.is_empty());
        if container.is_some() {
            output.push_line(
                "text matched inside the scroll container; find matches below are page-wide",
            );
        }
        output.data["matches_scope"] = json!("page");
        output.data["count"] = json!(matches.count);
        output.data["matches"] = json!(matches.lines);
        for line in matches.lines.into_iter().chain(matches.more) {
            output.push_line(&line);
        }
    }
    Ok(output)
}

fn cap_reason(steps: u64, expired: bool) -> Option<&'static str> {
    if steps >= MAX_STEPS {
        Some("60-step cap")
    } else if expired {
        Some("time cap (8 s or session deadline)")
    } else {
        None
    }
}

fn not_found_head(text: &str, steps: u64, reason: &str, capped: bool) -> String {
    let reason = if capped {
        format!("the {reason}; more may load")
    } else {
        reason.to_owned()
    };
    format!(
        "{} is not on the page after {steps} scrolls; stopped at {reason}",
        quote_text(text)
    )
}

fn found_head(text: &str, steps: u64, located: bool) -> String {
    if located {
        format!("found {} after {steps} scrolls:", quote_text(text))
    } else {
        format!(
            "{} is on the page after {steps} scrolls; use `tabref find` to locate it",
            quote_text(text)
        )
    }
}

fn end_summary(
    direction: &str,
    step: f64,
    steps: u64,
    total: &ScrollOutcome,
    (reason, capped): (&str, bool),
) -> String {
    let detail = super::first_line(direction, step, total);
    let suffix = if total.items > 0 {
        detail.split_once(": ").map_or("", |(_, tail)| tail)
    } else {
        ""
    };
    let mut head = if capped {
        format!("scrolled {direction} {steps} times, stopped at the {reason}")
    } else {
        format!("scrolled {direction} {steps} times to {reason}")
    };
    if !suffix.is_empty() {
        head.push_str(": ");
        head.push_str(suffix);
    } else if total.unwatched {
        head.push_str("; could not check what loaded");
    } else if total.hidden {
        head.push_str("; more may still be loading (tab not showing)");
    } else if total.loading {
        head.push_str("; more may still be loading");
    }
    if capped {
        head.push_str("; more may load");
    }
    head
}

fn stopped(outcome: &ScrollOutcome) -> bool {
    !outcome.moved
        && outcome.at_end
        && outcome.items == 0
        && !outcome.grew
        && !outcome.loading
        && !outcome.hidden
        && !outcome.unwatched
}

fn accumulate(total: &mut ScrollOutcome, next: ScrollOutcome) {
    let first = total.first.take().or_else(|| next.first.clone());
    let last = next.last.clone().or_else(|| total.last.take());
    let items = total.items + next.items;
    let moved = total.moved || next.moved;
    *total = ScrollOutcome {
        first,
        last,
        items,
        moved,
        ..next
    };
}

fn probe_until(
    session: &mut Session,
    container: Option<&ActTarget>,
    until: Option<&str>,
    deadline: Deadline,
) -> Result<bool, AppError> {
    let Some(text) = until else { return Ok(false) };
    let (page, site) = match container {
        Some(target) => (target.session.clone(), ScrollSite::Element(&target.token)),
        None => (session.page()?, ScrollSite::Page),
    };
    match site.contains_visible_text(session.connection(), &page, text, deadline) {
        Err(error) if is_timeout(&error) && deadline.is_expired() => Ok(false),
        result => result,
    }
}

#[cfg(test)]
#[path = "scroll_until_tests.rs"]
mod tests;
