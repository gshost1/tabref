use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, read_element, select_option};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// Picks the option `option` names in the select `target` names.
pub(crate) fn run(session: &mut Session, target: &str, option: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Select, deadline)?;
    let acted = act(session, target, option, deadline)?;
    full_report(session, acted)
}

/// Picks the `<select>` option `option` names in any form the view prints
/// it (label, `label (value)` or value; then ignoring case, then by a
/// unique start), reads the select's value back and reports the option
/// before and after.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    option: &str,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Select, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let chosen = select_option(
        session.connection(),
        &target.session,
        &target.token,
        option,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    let reading = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    );
    let verified = reading
        .as_ref()
        .is_ok_and(|reading| reading.value.as_deref() == Some(chosen.value.as_str()));
    let mut summary = match &chosen.previous {
        Some(previous) => format!(
            "{} {} -> {}",
            target.label,
            quote_text(previous),
            quote_text(&chosen.label)
        ),
        None => format!("{} = {}", target.label, quote_text(&chosen.label)),
    };
    if chosen.value != chosen.label {
        summary.push_str(&format!(" (value {})", quote_text(&chosen.value)));
    }
    if !verified {
        summary.push_str(" (the select did not keep it)");
    }
    Ok(Acted {
        verb: "select",
        summary,
        data: json!({
            "action": "select",
            "target": target.label,
            "value": chosen.value,
            "label": chosen.label,
            "previous": chosen.previous,
            "verified": verified,
        }),
        effects: Some(effects),
        known: target.known,
    })
}
