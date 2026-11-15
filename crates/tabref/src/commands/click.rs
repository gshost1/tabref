use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{LocateIntent, click};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Clicks the element `text` names, a ref or a locator, and reports what
/// the click set off.
pub(crate) fn run(session: &mut Session, text: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(text)?;
    let target = resolve_target(session, &spec, LocateIntent::Click, deadline)?;
    let acted = act(session, target, deadline)?;
    full_report(session, acted)
}

/// Clicks at a point that hit-tests to the element, then waits for what
/// the click set off to settle.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, LocateIntent::Click, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let point = click(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    Ok(Acted {
        verb: "click",
        data: json!({ "action": "click", "target": target.label, "point": point }),
        summary: target.label,
        effects: Some(effects),
        known: target.known,
    })
}
