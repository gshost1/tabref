use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, ElementReading, LocateIntent, append_text, read_element};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// Types `text` at the end of the field `target` names, keeping what it
/// holds, and reads it back.
pub(crate) fn run(session: &mut Session, target: &str, text: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Fill, deadline)?;
    let acted = act(session, target, text, deadline)?;
    full_report(session, acted)
}

/// Appends `text` to the field as one input edit after its current
/// content, as `fill` would type it but without clearing, and reports the
/// content before and after: `"Greeting" "Hello" -> "Hello world"`.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    text: &str,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Fill, deadline)?;
    let before = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    append_text(
        session.connection(),
        &target.session,
        &target.token,
        text,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    let after = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let previous = content(&before);
    let observed = content(&after);
    let verified = observed.trim_end_matches('\n').ends_with(text);
    let mut summary = format!(
        "{} {} -> {}",
        target.label,
        quote_text(&previous),
        quote_text(&observed)
    );
    if !verified {
        summary.push_str(&format!(" (asked to add {})", quote_text(text)));
    }
    Ok(Acted {
        verb: "type",
        summary,
        data: json!({
            "action": "type",
            "target": target.label,
            "previous": previous,
            "observed": observed,
            "typed": text,
            "verified": verified,
        }),
        effects: Some(effects),
        known: target.known,
    })
}

fn content(reading: &ElementReading) -> String {
    reading
        .value
        .clone()
        .or_else(|| reading.text.clone())
        .unwrap_or_default()
}
