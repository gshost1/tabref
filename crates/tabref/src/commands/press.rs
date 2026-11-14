use serde_json::json;
use tabref_core::{AppError, DeliverySemantics};
use tabref_page::{focus, press_key};

use super::action_report::finish_with_effects;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Presses `key` in the focused element, or in `target` after focusing it,
/// then reports what the key set off, such as a submitted form.
pub(crate) fn run(
    session: &mut Session,
    key: &str,
    target: Option<&str>,
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let (page, label, ref_text) = match target {
        Some(text) => {
            let target = session.resolve_ref(text)?;
            focus(
                session.connection(),
                &target.session,
                &target.token,
                deadline,
            )?;
            (
                target.session.clone(),
                format!(" in {}", target.label()),
                Some(target.ref_id.to_string()),
            )
        }
        None => (session.page()?, String::new(), None),
    };
    let tab = session.tab_id()?;
    session.watch_action(&tab)?;
    press_key(session.connection(), &page, key, deadline)?;
    let outcome = Outcome::new(
        format!("pressed {key}{label}"),
        json!({
            "action": "press",
            "key": key,
            "ref": ref_text,
            "disposition": DeliverySemantics::delivered_unverified(),
        }),
    );
    finish_with_effects(session, outcome)
}
