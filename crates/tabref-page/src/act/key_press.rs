use serde_json::json;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::key_definition::KEYS;
use super::key_stroke::KeyStroke;
use crate::{CdpConnection, PageSession};

/// Presses and releases one key in the page's focused element: a named key,
/// a single character, or either after `+`-joined modifiers such as
/// `Control+a`. Keys that type text also send the `char` event that inserts
/// it.
pub fn press_key(
    connection: &mut CdpConnection,
    session: &PageSession,
    key_name: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    let stroke = KeyStroke::parse(key_name).ok_or_else(|| unknown_key(key_name))?;
    let base = json!({
        "key": stroke.key,
        "code": stroke.code,
        "windowsVirtualKeyCode": stroke.key_code,
        "nativeVirtualKeyCode": stroke.key_code,
        "modifiers": stroke.modifiers,
    });
    let mut events = Vec::with_capacity(3);
    let mut down = base.clone();
    down["type"] = json!(if stroke.text.is_some() {
        "keyDown"
    } else {
        "rawKeyDown"
    });
    events.push(down);
    if let Some(text) = &stroke.text {
        let mut typed = base.clone();
        typed["type"] = json!("char");
        typed["text"] = json!(text);
        typed["unmodifiedText"] = json!(text);
        events.push(typed);
    }
    let mut up = base;
    up["type"] = json!("keyUp");
    events.push(up);
    for event in events {
        connection.call(
            "Input.dispatchKeyEvent",
            event,
            Some(&session.session_id),
            deadline,
        )?;
    }
    Ok(())
}

/// The key names [`press_key`] accepts besides single characters.
pub fn supported_key_names() -> Vec<&'static str> {
    KEYS.iter()
        .map(|definition| {
            if definition.key == " " {
                "Space"
            } else {
                definition.key
            }
        })
        .collect()
}

fn unknown_key(name: &str) -> AppError {
    AppError::new(ErrorCode::InvalidArgs, format!("Unsupported key '{name}'"))
        .with_suggestion(format!(
            "Use a single character, one of {}, or either after Control+, Shift+, Alt+ or Meta+",
            supported_key_names().join(", ")
        ))
        .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "key_press_tests.rs"]
mod tests;
