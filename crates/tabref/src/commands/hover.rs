use serde_json::json;
use tabref_core::{AppError, DeliverySemantics};
use tabref_page::hover;

use crate::output::outcome::Outcome;
use crate::session::Session;

/// Moves the pointer over the element, which opens hover menus and
/// tooltips; snap afterwards to read them.
pub(crate) fn run(session: &mut Session, target: &str) -> Result<Outcome, AppError> {
    let target = session.resolve_ref(target)?;
    let deadline = session.deadline();
    let point = hover(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    Ok(Outcome::new(
        format!("hovered {}", target.label()),
        json!({
            "action": "hover",
            "ref": target.ref_id.to_string(),
            "point": point,
            "disposition": DeliverySemantics::delivered_unverified(),
        }),
    ))
}
