use serde_json::{Value, json};

/// A state of one element that `wait --ref` and `expect T is S` wait for:
/// enabled (shown and not disabled), shown, no longer shown (`hidden` and
/// `gone` alike, removed or not), or holding a value (the field value of an
/// input, text area or select, the text of any other element).
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
    /// the name; one pair of matching quotes around X is not part of the
    /// value. Anything else is `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if let Some((name, value)) = text.split_once('=')
            && name.trim().eq_ignore_ascii_case("value")
        {
            return Some(Self::Value(unquoted(value).to_owned()));
        }
        match text.to_ascii_lowercase().as_str() {
            "enabled" => Some(Self::Enabled),
            "visible" => Some(Self::Visible),
            "hidden" => Some(Self::Hidden),
            "gone" => Some(Self::Gone),
            _ => None,
        }
    }

    /// Whether the state is the element no longer being shown, which an
    /// element that is not there at all has reached.
    pub fn absent(&self) -> bool {
        matches!(self, Self::Hidden | Self::Gone)
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

fn unquoted(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

#[cfg(test)]
#[path = "element_state_tests.rs"]
mod tests;
