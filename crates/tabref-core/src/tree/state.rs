//! The state tokens a node may carry.

pub const FOCUSED: &str = "focused";
pub const DISABLED: &str = "disabled";
pub const EXPANDED: &str = "expanded";
pub const CHECKED: &str = "checked";
pub const SELECTED: &str = "selected";
pub const HIDDEN: &str = "hidden";
pub const BUSY: &str = "busy";
pub const MODAL: &str = "modal";
pub const REQUIRED: &str = "required";
pub const INDETERMINATE: &str = "indeterminate";
pub const PRESSED: &str = "pressed";
pub const READONLY: &str = "readonly";
pub const INVALID: &str = "invalid";
pub const MULTISELECTABLE: &str = "multiselectable";
pub const HASPOPUP: &str = "haspopup";

pub const STATE_VOCABULARY: &[&str] = &[
    FOCUSED,
    DISABLED,
    EXPANDED,
    CHECKED,
    SELECTED,
    HIDDEN,
    BUSY,
    MODAL,
    REQUIRED,
    INDETERMINATE,
    PRESSED,
    READONLY,
    INVALID,
    MULTISELECTABLE,
    HASPOPUP,
];

pub fn has_state(states: &[String], token: &str) -> bool {
    states.iter().any(|state| state == token)
}

pub fn is_known_state(token: &str) -> bool {
    STATE_VOCABULARY.contains(&token)
}
