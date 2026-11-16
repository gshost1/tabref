use serde_json::Value;

/// What an action set off once it settled: whether the tab committed a new
/// document, whether it changed its address within the document (a
/// single-page app's route change), whether it sent a document, XHR or fetch
/// request, and the JavaScript dialogs that opened, each with its `type`,
/// `message`, the `url` of the frame that opened it, and the `answer` the
/// connection already gave it (`{accept, text}`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettleOutcome {
    pub navigated: bool,
    pub same_document: bool,
    pub network: bool,
    pub dialogs: Vec<Value>,
}
