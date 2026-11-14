use serde_json::json;
use tabref_core::{AppError, DeliverySemantics};
use tabref_page::{read_element, select_option};

use super::action_report::finish_with_effects;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::node_line::quote_text;

/// Picks the `<select>` option whose label, value or text is `option`, then
/// reads the select's value back.
pub(crate) fn run(session: &mut Session, target: &str, option: &str) -> Result<Outcome, AppError> {
    let target = session.resolve_ref(target)?;
    session.watch_action(&target.tab_id)?;
    let deadline = session.deadline();
    let value = select_option(
        session.connection(),
        &target.session,
        &target.token,
        option,
        deadline,
    )?;
    let reading = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let verified = reading.value.as_deref() == Some(value.as_str());
    let disposition = if verified {
        DeliverySemantics::delivered_verified()
    } else {
        DeliverySemantics::delivered_unverified()
    };
    let outcome = Outcome::new(
        format!(
            "selected {} in {} (value {})",
            quote_text(option),
            target.label(),
            quote_text(&value)
        ),
        json!({
            "action": "select",
            "ref": target.ref_id.to_string(),
            "value": value,
            "verified": verified,
            "disposition": disposition,
        }),
    );
    finish_with_effects(session, outcome)
}
