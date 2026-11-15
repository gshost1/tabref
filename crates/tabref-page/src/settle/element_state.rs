use serde_json::{Value, json};

/// A state of one element that `wait --ref` and `expect T is S` wait for:
/// enabled, shown, not shown, removed from its document, or holding a value
/// (its field value, or its text for an element that has none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementState {
    Enabled,
    Visible,
    Hidden,
    Gone,
    Value(String),
}

impl ElementState {
    /// `enabled`, `visible`, `hidden`, `gone` or `value=X`, ignoring case in
    /// the name; anything else is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some((name, value)) = text.split_once('=')
            && name.trim().eq_ignore_ascii_case("value")
        {
            return Some(Self::Value(value.to_owned()));
        }
        match text.to_ascii_lowercase().as_str() {
            "enabled" => Some(Self::Enabled),
            "visible" | "shown" => Some(Self::Visible),
            "hidden" => Some(Self::Hidden),
            "gone" => Some(Self::Gone),
            _ => None,
        }
    }

    /// How a result names the state: `enabled`, `value "62.00"`.
    pub fn describe(&self) -> String {
        match self {
            Self::Enabled => "enabled".to_owned(),
            Self::Visible => "visible".to_owned(),
            Self::Hidden => "hidden".to_owned(),
            Self::Gone => "gone".to_owned(),
            Self::Value(value) => format!("value {value:?}"),
        }
    }

    /// The state as the in-page check reads it.
    pub(crate) fn wire(&self) -> Value {
        match self {
            Self::Enabled => json!({ "state": "enabled" }),
            Self::Visible => json!({ "state": "visible" }),
            Self::Hidden => json!({ "state": "hidden" }),
            Self::Gone => json!({ "state": "gone" }),
            Self::Value(value) => json!({ "state": "value", "value": value }),
        }
    }
}

#[cfg(test)]
#[path = "element_state_tests.rs"]
mod tests;
