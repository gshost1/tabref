use serde_json::json;
use tabref_core::{AppError, DeliverySemantics};
use tabref_page::click;

use super::action_report::finish_with_effects;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Clicks the element at a point that hit-tests to it, then reports what
/// the click set off.
pub(crate) fn run(session: &mut Session, target: &str) -> Result<Outcome, AppError> {
    let target = session.resolve_ref(target)?;
    session.watch_action(&target.tab_id)?;
    let deadline = session.deadline();
    let point = click(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let outcome = Outcome::new(
        format!("clicked {}", target.label()),
        json!({
            "action": "click",
            "ref": target.ref_id.to_string(),
            "point": point,
            "disposition": DeliverySemantics::delivered_unverified(),
        }),
    );
    finish_with_effects(session, outcome)
}
