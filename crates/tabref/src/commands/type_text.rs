use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{ActionNeed, LocateIntent, append_text};

use super::act_target::{ActTarget, resolve_target};
use super::acted::Acted;
use super::action_report::full_report;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

const SHOWN_TAIL: usize = 40;

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
/// end of the content before and after: `"Greeting" "Hello" -> "Hello
/// world"`, with a long field cut to its last 40 characters.
pub(crate) fn act(
    session: &mut Session,
    target: ActTarget,
    text: &str,
    deadline: Deadline,
) -> Result<Acted, AppError> {
    target.ensure_ready(session, ActionNeed::Fill, deadline)?;
    let watched = session.watch_action(&target.tab_id, deadline)?;
    let typed = append_text(
        session.connection(),
        &target.session,
        &target.token,
        text,
        deadline,
    )?;
    let effects = session.finish_action(&watched, deadline)?;
    let previous = tail(&typed.previous);
    let observed = tail(&typed.observed);
    let mut summary = format!(
        "{} {} -> {}",
        target.label,
        quote_text(&previous),
        quote_text(&observed)
    );
    if !typed.verified {
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
            "previous_chars": typed.previous.chars().count(),
            "observed_chars": typed.observed.chars().count(),
            "typed": text,
            "verified": typed.verified,
        }),
        effects: Some(effects),
        known: target.known,
    })
}

/// The last 40 characters of `text`, after an ellipsis when it is longer.
pub(crate) fn tail(text: &str) -> String {
    let count = text.chars().count();
    if count <= SHOWN_TAIL {
        return text.to_owned();
    }
    let kept: String = text.chars().skip(count - SHOWN_TAIL).collect();
    format!("…{kept}")
}

#[cfg(test)]
#[path = "type_text_tests.rs"]
mod tests;
