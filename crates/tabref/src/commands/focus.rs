use std::time::Duration;

use serde_json::json;
use tabref_core::{AppError, Deadline, ErrorCode};
use tabref_page::{
    ActionNeed, LocateIntent, emulate_focus, focus, focus_target, holds_focus, page_focused,
};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

const MOVE_GRACE: Duration = Duration::from_millis(60);
const MOVE_POLL: Duration = Duration::from_millis(20);

/// Moves the keyboard focus to the element `target` names and reports what
/// the page did on focus. A name finds a field first, so a field wins over
/// its own label, then any control.
pub(crate) fn run(session: &mut Session, target: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = match resolve_target(session, &spec, LocateIntent::Fill, deadline) {
        Err(error) if error.code == ErrorCode::ElementNotFound => {
            resolve_target(session, &spec, LocateIntent::Click, deadline)?
        }
        found => found?,
    };
    let acted = act(session, target, deadline)?;
    full_report(session, acted)
}

/// Focuses the element (a label's control for a label) once the gate finds
/// it shown and enabled and it can take focus; nothing in the page changes
/// before that. A tab that is in the background or lacks the window focus
/// is told it has it for this command only, before the watch starts, so
/// its own visibility and focus events stay out of the report, and is told
/// so no longer when the command ends. A page that moves the focus away,
/// even from a short timer, is reported: the focus is checked after the
/// action settled and for 60 ms more.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Press, deadline)?;
    let token = focus_target(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let emulated = !page_focused(session.connection(), &target.session, deadline)?;
    if emulated {
        emulate_focus(session.connection(), &target.session, true, deadline)?;
    }
    let focused = focus_and_settle(session, &target, &token, deadline);
    if emulated {
        let _ = emulate_focus(session.connection(), &target.session, false, deadline);
    }
    let (effects, held) = focused?;
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

fn focus_and_settle(
    session: &mut Session,
    target: &ActTarget,
    token: &tabref_page::ElementToken,
    deadline: Deadline,
) -> Result<(crate::session::action_effects::ActionEffects, bool), AppError> {
    let watched = session.watch_action(&target.tab_id, deadline)?;
    focus(session.connection(), &target.session, token, deadline)?;
    let effects = session.finish_action(&watched, deadline)?;
    let grace = deadline.capped(MOVE_GRACE);
    loop {
        let held = holds_focus(session.connection(), &target.session, token, deadline)?;
        if !held || grace.is_expired() {
            return Ok((effects, held));
        }
        std::thread::sleep(MOVE_POLL.min(grace.remaining()));
    }
}
