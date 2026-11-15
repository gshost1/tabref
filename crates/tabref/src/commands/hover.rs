use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{LocateIntent, hover};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Moves the pointer over the element `target` names.
pub(crate) fn run(session: &mut Session, target: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Hover, deadline)?;
    let acted = act(session, target, deadline)?;
    full_report(session, acted)
}

/// Hovers the element, which opens hover menus and tooltips, and reports
/// the controls that appeared.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, LocateIntent::Hover, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let point = hover(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    Ok(Acted {
        verb: "hover",
        data: json!({ "action": "hover", "target": target.label, "point": point }),
        summary: target.label,
        effects: Some(effects),
        known: target.known,
    })
}
