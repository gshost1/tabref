use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{LocateIntent, scroll_by, scroll_into_view, scroll_page};

use super::act_target::resolve_target;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

const PAGE_STEP: u32 = 600;

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
    let moved = match container {
        Some(spec) => {
            let target = resolve_target(session, spec, LocateIntent::Click, deadline)?;
            scroll_by(
                session.connection(),
                &target.session,
                &target.token,
                delta,
                deadline,
            )?
        }
        None => {
            let page = session.page()?;
            scroll_page(session.connection(), &page, delta, deadline)?
        }
    };
    let text = if moved {
        format!("scrolled {direction} {step}px")
    } else {
        format!("did not move: nothing left to scroll {direction}")
    };
    Ok(Outcome::new(
        text,
        json!({ "action": "scroll", "direction": direction, "px": step, "moved": moved }),
    ))
}
