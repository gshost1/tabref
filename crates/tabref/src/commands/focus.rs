use std::time::Duration;

use serde_json::json;
use tabref_core::{AppError, Deadline, ErrorCode};
use tabref_page::{
    ActionNeed, FocusTarget, LocateIntent, focus, focus_kept, focus_target, page_focused,
    release_focus_watch, wait_actionable, watch_focus,
};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::action_effects::ActionEffects;
use crate::session::action_watch::after_delivery;

const MOVE_WINDOW: Duration = Duration::from_millis(300);
const MOVED_AWAY: &str = " (the page moved the focus away)";
const IN_BACKGROUND: &str =
    " (the tab is in the background: the page may not run its focus handlers)";

/// Moves the keyboard focus to the element `target` names and reports what
/// the page did on focus. A name finds a field first, so a field wins over
/// its own label, then any control.
pub(crate) fn run(session: &mut Session, target: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = field_then_control(|intent| resolve_target(session, &spec, intent, deadline))?;
    let acted = act(session, target, deadline)?;
    full_report(session, acted)
}

/// The target `find` finds for a focus, as a command or a `do` step: a
/// field first, so a field wins over its own label, then any control.
pub(crate) fn field_then_control(
    mut find: impl FnMut(LocateIntent) -> Result<ActTarget, AppError>,
) -> Result<ActTarget, AppError> {
    match find(LocateIntent::Fill) {
        Err(error) if error.code == ErrorCode::ElementNotFound => find(LocateIntent::Click),
        found => found,
    }
}

/// Focuses the element (a label's control for a label) once the gate finds
/// it, and a label's control too, shown and enabled, and it can take focus;
/// nothing in the page changes before that. Only `DOM.focus` is sent: the
/// tab's window focus is never changed here. A tab of a browser tabref
/// launched was told at attach that it holds the window focus; in any other
/// browser a tab that is in the background or lacks the window focus is
/// focused all the same and said to be in the background, as its page may
/// not run its focus handlers. A page that moves the focus away within
/// 300 ms of the focus is reported; with no focus listener on the
/// element's path, the focus is checked once the action settled.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Press, deadline)?;
    let chosen = focus_target(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let token = &chosen.token;
    if *token != target.token
        && let Some(blocked) = wait_actionable(
            session.connection(),
            &target.session,
            token,
            ActionNeed::Press,
            deadline,
        )?
    {
        return Err(blocked.error(&target.subject));
    }
    let background = !session.launched_by_tabref()
        && !page_focused(session.connection(), &target.session, deadline)?;
    let (effects, held) = focus_and_settle(session, &target, &chosen, deadline)?;
    let mut summary = target.label.clone();
    if !held {
        summary.push_str(MOVED_AWAY);
    }
    if background {
        summary.push_str(IN_BACKGROUND);
    }
    Ok(Acted {
        verb: "focus",
        data: json!({
            "action": "focus",
            "target": target.label,
            "focused": held,
            "background": background,
        }),
        summary,
        effects: Some(effects),
        known: target.known,
    })
}

/// Focuses `chosen`, starts the page's watch on the focus at once, and
/// reads the watch once the action settled, so the 300 ms window and the
/// settle run together. A settle that fails releases the watch first.
fn focus_and_settle(
    session: &mut Session,
    target: &ActTarget,
    chosen: &FocusTarget,
    deadline: Deadline,
) -> Result<(ActionEffects, bool), AppError> {
    let token = &chosen.token;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    focus(session.connection(), &target.session, chosen, deadline)?;
    let watch = watch_focus(
        session.connection(),
        &target.session,
        token,
        MOVE_WINDOW,
        deadline,
    )?;
    let effects = match session.finish_action(&watched, deadline) {
        Ok(effects) => effects,
        Err(error) => {
            release_focus_watch(session.connection(), &target.session, deadline);
            return Err(error);
        }
    };
    let held = focus_kept(
        session.connection(),
        &target.session,
        token,
        watch,
        deadline,
    )
    .map_err(after_delivery)?;
    Ok((effects, held))
}
