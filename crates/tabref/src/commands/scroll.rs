use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{
    LocateIntent, ScrollOutcome, ScrollSite, cut_chars, scroll_and_settle, scroll_into_view,
};

use super::act_target::resolve_target;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

const PAGE_STEP: u32 = 600;
const LINE_CHARS: usize = 100;
const ITEM_CHARS: usize = 40;

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
    let outcome = match container {
        Some(spec) => {
            let target = resolve_target(session, spec, LocateIntent::Click, deadline)?;
            scroll_and_settle(
                session.connection(),
                &target.session,
                ScrollSite::Element(&target.token),
                delta,
                deadline,
            )?
        }
        None => {
            let page = session.page()?;
            scroll_and_settle(
                session.connection(),
                &page,
                ScrollSite::Page,
                delta,
                deadline,
            )?
        }
    };
    let mut report = Outcome::new(
        first_line(direction, step, &outcome),
        json!({
            "action": "scroll",
            "direction": direction,
            "px": step,
            "moved": outcome.moved,
            "at_end": outcome.at_end,
            "new_items": outcome.items,
            "new_lines": outcome.lines,
            "loading": outcome.loading,
        }),
    );
    for line in &outcome.lines {
        report.push_line(&format!("+ {}", cut_chars(line, LINE_CHARS)));
    }
    if outcome.more_lines > 0 {
        report.push_line(&format!(
            "+{} more new lines: tabref find TEXT",
            outcome.more_lines
        ));
    }
    Ok(report)
}

/// `scrolled down 600px`, with what the scroll loaded or that the list
/// ended: `: 20 new items loaded ("Entry 31" to "Entry 50")`, `; end of
/// list: nothing more loaded`, `; more is still loading`.
fn first_line(direction: &str, step: f64, outcome: &ScrollOutcome) -> String {
    let end = if matches!(direction, "up" | "left") {
        "top of list"
    } else {
        "end of list"
    };
    let moved = format!("scrolled {direction} {step}px");
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
        let lead = if outcome.moved {
            format!("{moved}: ")
        } else {
            "did not move, then ".to_owned()
        };
        let still = if outcome.loading {
            "; more is still loading"
        } else {
            ""
        };
        return format!("{lead}{items} loaded{span}{still}");
    }
    match (outcome.moved, outcome.loading, outcome.at_end) {
        (true, true, _) => format!("{moved}; more is still loading"),
        (false, true, _) => "did not move: more is still loading".to_owned(),
        (true, false, true) => format!("{moved}; {end}: nothing more loaded"),
        (false, false, true) => format!("did not move: {end}, nothing more loaded"),
        (true, false, false) => moved,
        (false, false, false) => format!("did not move: nothing left to scroll {direction}"),
    }
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;
