use serde_json::{Value, json};

/// What an action needs of its element before any input is sent. Every
/// action needs it attached and shown. A press of the pointer (click,
/// check, hover) also needs a box on screen to press. A checkbox or radio
/// button may lend its box from a label that draws it. Everything but a
/// hover and a plain show needs it enabled, and a fill needs a field that is
/// not read-only. A fill, a select and a key press reach their element
/// through focus or its value, so they need no box: a custom dropdown's
/// zero-size `<select>` or a one-time-code input still takes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionNeed {
    Click,
    Check,
    Hover,
    Fill,
    Select,
    Press,
    Shown,
}

impl ActionNeed {
    /// The requirements the in-page check reads.
    pub(crate) fn spec(self) -> Value {
        let boxed = matches!(self, Self::Click | Self::Check | Self::Hover);
        let enabled = !matches!(self, Self::Hover | Self::Shown);
        json!({
            "box": boxed,
            "enabled": enabled,
            "editable": self == Self::Fill,
            "label": self == Self::Check,
        })
    }
}

#[cfg(test)]
#[path = "action_need_tests.rs"]
mod tests;
