use std::borrow::Cow;

use serde::Deserialize;
use serde_json::Value;

/// A CDP `AXValue`; only its `value` member is read.
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct AxValue {
    #[serde(default)]
    pub(crate) value: Option<Value>,
}

impl AxValue {
    pub(crate) fn as_text(&self) -> Option<String> {
        self.trimmed_text().map(Cow::into_owned)
    }

    pub(crate) fn trimmed_text(&self) -> Option<Cow<'_, str>> {
        let text = match self.value.as_ref()? {
            Value::String(text) => Cow::Borrowed(text.trim()),
            Value::Bool(flag) => Cow::Borrowed(if *flag { "true" } else { "false" }),
            Value::Number(number) => Cow::Owned(number.to_string()),
            _ => return None,
        };
        (!text.is_empty()).then_some(text)
    }

    /// The string as the browser reported it, keeping the spaces at its ends
    /// that separate it from the text around it.
    pub(crate) fn as_untrimmed_text(&self) -> Option<String> {
        match self.value.as_ref()? {
            Value::String(text) => Some(text.clone()),
            _ => self.as_text(),
        }
    }

    pub(crate) fn is_true(&self) -> bool {
        match self.value.as_ref() {
            Some(Value::Bool(flag)) => *flag,
            Some(Value::String(text)) => text == "true",
            _ => false,
        }
    }
}
