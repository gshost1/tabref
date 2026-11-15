use serde_json::Value;

/// A `do` step that finished: what it prints (its `ok` line, and below it
/// any text the step reads out) and its `--json` data.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepDone {
    pub(crate) text: String,
    pub(crate) data: Value,
}
