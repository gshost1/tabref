use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, fill};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

const PREVIEW_EDGE: usize = 32;

/// Replaces the text of the field `target` names and reads it back.
pub(crate) fn run(session: &mut Session, target: &str, text: &str) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Fill, deadline)?;
    let acted = act(session, target, text, deadline)?;
    full_report(session, acted)
}

/// Fills the field with `text` (a date, time or number input takes it in a
/// natural form and is set whole) and reports the value before and after:
/// `"Price ($)" "67.00" -> "62.00"`. Long values print their length and
/// a preview of both ends.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    text: &str,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Fill, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let filled = fill(
        session.connection(),
        &target.session,
        &target.token,
        text,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    let observed = filled.observed.as_deref().unwrap_or_default();
    let mut summary = match &filled.previous {
        Some(previous) => format!(
            "{} {} -> {}",
            target.label,
            quote_value(previous),
            quote_value(observed)
        ),
        None => format!("{} = {}", target.label, quote_value(observed)),
    };
    if !filled.verified {
        summary.push_str(&format!(" (asked for {})", quote_value(text)));
    }
    Ok(Acted {
        verb: "fill",
        summary,
        data: json!({
            "action": "fill",
            "target": target.label,
            "previous": filled.previous,
            "observed": filled.observed,
            "verified": filled.verified,
            "input_type": filled.assigned.as_ref().map(|input| input.input_type.clone()),
        }),
        effects: Some(effects),
        known: target.known,
    })
}

fn quote_value(text: &str) -> String {
    let len = text.chars().count();
    if len <= PREVIEW_EDGE * 2 {
        return quote_text(text);
    }
    let head: String = text.chars().take(PREVIEW_EDGE).collect();
    let tail: String = text.chars().skip(len - PREVIEW_EDGE).collect();
    format!("{} ({len} chars)", quote_text(&format!("{head}…{tail}")))
}
