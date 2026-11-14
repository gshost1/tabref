use serde_json::Value;

/// What an action set off: whether its tab navigated, the dialogs it opened
/// (already accepted), and the tab it opened, which is now the current one.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ActionEffects {
    pub(crate) navigated: bool,
    pub(crate) dialogs: Vec<Value>,
    pub(crate) new_tab: Option<String>,
}
