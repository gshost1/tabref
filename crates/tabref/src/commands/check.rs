use serde_json::json;
use tabref_core::{AppError, DeliverySemantics};
use tabref_page::{read_element, set_checked};

use super::action_report::finish_with_effects;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Sets a checkbox, radio button or switch to `checked`, clicking it only
/// when its state differs, and confirms the state afterwards.
pub(crate) fn run(session: &mut Session, target: &str, checked: bool) -> Result<Outcome, AppError> {
    let target = session.resolve_ref(target)?;
    let deadline = session.deadline();
    let before = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let verb = if checked { "checked" } else { "unchecked" };
    if before.checked == Some(checked) {
        return Ok(Outcome::new(
            format!("{} was already {verb}", target.label()),
            json!({
                "action": verb,
                "ref": target.ref_id.to_string(),
                "checked": checked,
                "disposition": DeliverySemantics::not_delivered(),
            }),
        ));
    }
    session.watch_action(&target.tab_id)?;
    let state = set_checked(
        session.connection(),
        &target.session,
        &target.token,
        checked,
        deadline,
    )?;
    let outcome = Outcome::new(
        format!("{verb} {}", target.label()),
        json!({
            "action": verb,
            "ref": target.ref_id.to_string(),
            "checked": state,
            "disposition": DeliverySemantics::delivered_verified(),
        }),
    );
    finish_with_effects(session, outcome)
}
