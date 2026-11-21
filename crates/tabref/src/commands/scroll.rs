use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{
    LocateIntent, ScrollOutcome, ScrollSite, cut_chars, scroll_and_settle, scroll_into_view,
};

use super::act_target::{ActTarget, resolve_target};
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::{escape_line, quote_text};

const PAGE_STEP: u32 = 600;
const LINE_CHARS: usize = 100;
const ITEM_CHARS: usize = 40;

#[path = "scroll_until.rs"]
mod scroll_until;
pub(crate) use scroll_until::run_loop;

#[path = "scroll_batch.rs"]
mod scroll_batch;
#[cfg(test)]
pub(crate) use scroll_batch::parse_batch_scroll;
pub(crate) use scroll_batch::run_batch_loop;

/// Scrolls the page, or the container that holds `target`, in `direction`;
/// with a target in place of the direction, scrolls that element into view.
pub(crate) fn run(
    session: &mut Session,
    direction: &str,
    target: Option<&str>,
    px: Option<u32>,
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let container = match target {
        Some(text) => Some(TargetSpec::from_arg(text)?),
        None => None,
    };
    scroll(session, direction, container.as_ref(), px, deadline)
}

/// [`run`] with the container already read, as a `do` step gives it.
pub(crate) fn scroll(
    session: &mut Session,
    direction: &str,
    container: Option<&TargetSpec>,
    px: Option<u32>,
    deadline: Deadline,
) -> Result<Outcome, AppError> {
    let step = f64::from(px.unwrap_or(PAGE_STEP));
    let delta = match direction.trim().to_ascii_lowercase().as_str() {
        "down" => (0.0, step),
        "up" => (0.0, -step),
        "right" => (step, 0.0),
        "left" => (-step, 0.0),
        _ => {
            let spec = TargetSpec::from_arg(direction)?;
            let target = resolve_target(session, &spec, LocateIntent::Click, deadline)?;
            scroll_into_view(
                session.connection(),
                &target.session,
                &target.token,
                deadline,
            )?;
            return Ok(Outcome::new(
                format!("scrolled {} into view", target.label),
                json!({ "action": "scroll", "target": target.label, "moved": true }),
            ));
        }
    };
    let target = container
        .map(|spec| resolve_target(session, spec, LocateIntent::Click, deadline))
        .transpose()?;
    let outcome = step_outcome(session, target.as_ref(), delta, deadline)?;
    Ok(report(direction, step, &outcome))
}

fn step_outcome(
    session: &mut Session,
    container: Option<&ActTarget>,
    delta: (f64, f64),
    deadline: Deadline,
) -> Result<ScrollOutcome, AppError> {
    match container {
        Some(target) => {
            scroll_and_settle(
                session.connection(),
                &target.session,
                ScrollSite::Element(&target.token),
                delta,
                deadline,
            )
        }
        None => {
            let page = session.page()?;
            scroll_and_settle(
                session.connection(),
                &page,
                ScrollSite::Page,
                delta,
                deadline,
            )
        }
    }
}

fn report(direction: &str, step: f64, outcome: &ScrollOutcome) -> Outcome {
    let mut report = Outcome::new(
        first_line(direction, step, outcome),
        json!({
            "action": "scroll",
            "direction": direction,
            "px": step,
            "moved": outcome.moved,
            "at_end": outcome.at_end,
            "new_items": outcome.items,
            "new_lines": outcome.lines,
            "loading": outcome.loading,
            "hidden": outcome.hidden,
            "unwatched": outcome.unwatched,
        }),
    );
    for line in &outcome.lines {
        report.push_line(&format!(
            "+ {}",
            escape_line(&cut_chars(line, LINE_CHARS), false)
        ));
    }
    if outcome.more_lines > 0 {
        report.push_line(&format!(
            "+{} more new lines: tabref find TEXT",
            outcome.more_lines
        ));
    }
    report
}

/// `scrolled down 600px`, with what the scroll loaded or why it stopped:
/// `: 20 new items loaded ("Entry 31" to "Entry 50")`, `; at the bottom:
/// nothing new loaded`, `; more may still be loading`. tabref claims no
/// more than it saw: at the bottom with a request still running, a DOM
/// still changing, a tab that draws no frames or a watch an error stopped,
/// it says so instead.
fn first_line(direction: &str, step: f64, outcome: &ScrollOutcome) -> String {
    let edge = match direction {
        "up" => "at the top",
        "left" => "at the left end",
        "right" => "at the right end",
        _ => "at the bottom",
    };
    let moved = format!("scrolled {direction} {step}px");
    let doubt = if outcome.unwatched {
        "; could not check what loaded"
    } else if outcome.hidden {
        "; more may still be loading (tab not showing)"
    } else if outcome.loading {
        "; more may still be loading"
    } else {
        ""
    };
    if outcome.items > 0 {
        let span = match (&outcome.first, &outcome.last) {
            (Some(first), Some(last)) if outcome.items > 1 => format!(
                " ({} to {})",
                quote_text(&cut_chars(first, ITEM_CHARS)),
                quote_text(&cut_chars(last, ITEM_CHARS))
            ),
            (Some(first), _) => format!(" ({})", quote_text(&cut_chars(first, ITEM_CHARS))),
            _ => String::new(),
        };
        let items = match outcome.items {
            1 => "1 new item".to_owned(),
            count => format!("{count} new items"),
        };
        return format!("{moved}: {items} loaded{span}{doubt}");
    }
    match (outcome.moved, outcome.at_end) {
        (true, _) if !doubt.is_empty() => format!("{moved}{doubt}"),
        (true, true) => format!("{moved}; {edge}: nothing new loaded"),
        (true, false) => moved,
        (false, true) if !doubt.is_empty() => format!("did not move: {edge}{doubt}"),
        (false, true) => format!("did not move: {edge}, nothing new loaded"),
        (false, false) => format!("did not move: nothing left to scroll {direction}"),
    }
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;
