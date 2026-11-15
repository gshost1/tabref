use serde_json::Value;

/// What a request in flight was: its method, for error messages; the
/// flattened session it went to, so a detach can fail it at once; and its
/// params when the session ledger records the request's effect; and whether
/// the caller wants the reply frame's raw text instead of a parsed value.
#[derive(Debug, Clone)]
pub(crate) struct Waiter {
    pub(crate) method: String,
    pub(crate) session_id: Option<String>,
    pub(crate) params: Option<Value>,
    pub(crate) raw: bool,
}
