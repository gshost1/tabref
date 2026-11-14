//! The action names a node advertises in `available_actions`.

pub const CLICK: &str = "Click";
pub const RIGHT_CLICK: &str = "RightClick";
pub const SET_VALUE: &str = "SetValue";
pub const SET_FOCUS: &str = "SetFocus";
pub const SELECT: &str = "Select";
pub const TOGGLE: &str = "Toggle";
pub const SCROLL_TO: &str = "ScrollTo";
pub const PRESS_KEY: &str = "PressKey";
pub const TYPE_TEXT: &str = "TypeText";

pub fn contains(actions: &[String], capability: &str) -> bool {
    actions.iter().any(|action| action == capability)
}
