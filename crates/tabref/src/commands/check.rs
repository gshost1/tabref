use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, read_element, set_checked};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Ticks or clears the checkbox, radio button or switch `target` names.
pub(crate) fn run(session: &mut Session, target: &str, checked: bool) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Check, deadline)?;
    let acted = act(session, target, checked, deadline)?;
    full_report(session, acted)
}

/// Sets the control to `checked`, clicking it only when its state differs,
/// and confirms the state afterwards.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    checked: bool,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    let verb = if checked { "check" } else { "uncheck" };
    let before = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    if before.checked == Some(checked) {
        return Ok(Acted {
            verb,
            summary: format!("{} (already {verb}ed)", target.label),
            data: json!({ "action": verb, "target": target.label, "checked": checked, "changed": false }),
            effects: None,
            known: target.known,
        });
    }
    target.ensure_ready(session, ActionNeed::Check, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let state = set_checked(
        session.connection(),
        &target.session,
        &target.token,
        checked,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    Ok(Acted {
        verb,
        data: json!({ "action": verb, "target": target.label, "checked": state, "changed": true }),
        summary: target.label,
        effects: Some(effects),
        known: target.known,
    })
}
