use serde_json::json;
use tabref_core::AppError;
use tabref_page::fill;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// Replaces the field's text and reports what it holds afterwards.
pub(crate) fn run(session: &mut Session, target: &str, text: &str) -> Result<Outcome, AppError> {
    let target = session.resolve_ref(target)?;
    let deadline = session.deadline();
    let filled = fill(
        session.connection(),
        &target.session,
        &target.token,
        text,
        deadline,
    )?;
    let observed = filled.observed.clone().unwrap_or_default();
    let line = if filled.verified {
        format!("filled {} = {}", target.label(), quote_text(&observed))
    } else {
        format!(
            "filled {} but it holds {}",
            target.label(),
            quote_text(&observed)
        )
    };
    Ok(Outcome::new(
        line,
        json!({
            "action": "fill",
            "ref": target.ref_id.to_string(),
            "observed": filled.observed,
            "verified": filled.verified,
            "input_type": filled.assigned.as_ref().map(|input| input.input_type.clone()),
            "disposition": filled.delivery(),
        }),
    ))
}
