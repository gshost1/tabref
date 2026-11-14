use serde_json::Value;

/// What a command prints: compact text for the default output, and the
/// `data` of the `--json` envelope.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Outcome {
    pub(crate) text: String,
    pub(crate) data: Value,
}

impl Outcome {
    pub(crate) fn new(text: impl Into<String>, data: Value) -> Self {
        Self {
            text: text.into(),
            data,
        }
    }

    /// Appends `more` to the text on a new line.
    pub(crate) fn push_line(&mut self, more: &str) {
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str(more);
    }
}
