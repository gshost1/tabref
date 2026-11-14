use serde_json::Value;

/// What an action set off once it settled: whether the page navigated, and
/// the JavaScript dialogs that opened, each with its `type` and `message`.
/// The connection already accepted those dialogs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettleOutcome {
    pub navigated: bool,
    pub dialogs: Vec<Value>,
}
