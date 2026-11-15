use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, focus_element};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Moves the keyboard focus to the element `target` names and reports what
/// the page did on focus.
pub(crate) fn run(session: &mut Session, target: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Click, deadline)?;
    let acted = act(session, target, deadline)?;
    full_report(session, acted)
}

/// Focuses the element once the gate finds it shown and enabled, and says
/// so when the page moved the focus away at once.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Press, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let held = focus_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    let mut summary = target.label.clone();
    if !held {
        summary.push_str(" (the page moved the focus away)");
    }
    Ok(Acted {
        verb: "focus",
        data: json!({ "action": "focus", "target": target.label, "focused": held }),
        summary,
        effects: Some(effects),
        known: target.known,
    })
}
