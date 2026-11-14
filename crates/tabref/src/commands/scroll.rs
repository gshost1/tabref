use serde_json::json;
use tabref_core::{AppError, RefId};
use tabref_page::{scroll_by, scroll_into_view, scroll_page};

use crate::output::outcome::Outcome;
use crate::session::Session;

const PAGE_STEP: u32 = 600;

/// Scrolls the page, or the container that holds `target`, in `direction`;
/// with a ref in place of the direction, scrolls that element into view.
pub(crate) fn run(
    session: &mut Session,
    direction: &str,
    target: Option<&str>,
    px: Option<u32>,
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    if RefId::looks_like_ref(direction) {
        let target = session.resolve_ref(direction)?;
        scroll_into_view(
            session.connection(),
            &target.session,
            &target.token,
            deadline,
        )?;
        return Ok(Outcome::new(
            format!("scrolled {} into view", target.label()),
            json!({ "action": "scroll", "ref": target.ref_id.to_string(), "moved": true }),
        ));
    }
    let step = f64::from(px.unwrap_or(PAGE_STEP));
    let delta = match direction.trim().to_ascii_lowercase().as_str() {
        "down" => (0.0, step),
        "up" => (0.0, -step),
        "right" => (step, 0.0),
        "left" => (-step, 0.0),
        other => {
            return Err(
                AppError::invalid_args(format!("'{other}' is not a direction"))
                    .with_suggestion("Use down, up, left or right, or a ref to scroll into view"),
            );
        }
    };
    let moved = match target {
        Some(text) => {
            let target = session.resolve_ref(text)?;
            scroll_by(
                session.connection(),
                &target.session,
                &target.token,
                delta,
                deadline,
            )?
        }
        None => {
            let page = session.page()?;
            scroll_page(session.connection(), &page, delta, deadline)?
        }
    };
    let text = if moved {
        format!("scrolled {direction} {step}px")
    } else {
        format!("did not move: nothing left to scroll {direction}")
    };
    Ok(Outcome::new(
        text,
        json!({ "action": "scroll", "direction": direction, "px": step, "moved": moved }),
    ))
}
