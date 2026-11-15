use std::collections::HashSet;

use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, focus, press_key};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Presses `key` in the focused element, or in `target` after focusing it.
pub(crate) fn run(
    session: &mut Session,
    key: &str,
    target: Option<&str>,
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let target = match target {
        Some(text) => Some(resolve_target(
            session,
            &TargetSpec::from_arg(text)?,
            LocateIntent::Click,
            deadline,
        )?),
        None => None,
    };
    let acted = act(session, key, target, deadline)?;
    full_report(session, acted)
}

/// Presses `key` (Enter, Tab, Escape, ArrowDown, Control+a, ...) and
/// reports what it set off, such as a submitted form.
pub(crate) fn act(
    session: &mut Session,
    key: &str,
    target: Option<ActTarget>,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    let (page, tab, label, known) = match target {
        Some(target) => {
            target.ensure_ready(session, ActionNeed::Press, deadline)?;
            focus(
                session.connection(),
                &target.session,
                &target.token,
                deadline,
            )?;
            let label = format!(" in {}", target.label);
            (target.session, target.tab_id, label, target.known)
        }
        None => {
            let page = session.page()?;
            let tab = page.target_id.clone();
            (page, tab, String::new(), HashSet::new())
        }
    };
    let watched = session.watch_action(&tab, deadline)?;
    press_key(session.connection(), &page, key, deadline)?;
    let effects = session.finish_action(&watched, deadline)?;
    Ok(Acted {
        verb: "press",
        summary: format!("{key}{label}"),
        data: json!({ "action": "press", "key": key, "target": label.trim_start_matches(" in ") }),
        effects: Some(effects),
        known,
    })
}
