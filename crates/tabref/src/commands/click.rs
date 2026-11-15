use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, click_times};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Clicks the element `text` names, a ref or a locator, `count` times (2 is
/// a double-click), and reports what the clicks set off.
pub(crate) fn run(session: &mut Session, text: &str, count: u32) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(text)?;
    let target = resolve_target(session, &spec, LocateIntent::Click, deadline)?;
    let acted = act(session, target, count, deadline)?;
    full_report(session, acted)
}

/// Clicks `count` times at one point that hit-tests to the element, each
/// press carrying its running click count so two make the page's
/// `dblclick`, then waits for what the clicks set off to settle.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    count: u32,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Click, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let point = click_times(
        session.connection(),
        &target.session,
        &target.token,
        count,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    let verb = if count >= 2 { "double-click" } else { "click" };
    Ok(Acted {
        verb,
        data: json!({ "action": verb, "target": target.label, "point": point, "count": count }),
        summary: target.label,
        effects: Some(effects),
        known: target.known,
    })
}
