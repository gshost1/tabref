use std::collections::HashSet;

use serde_json::Value;

use crate::session::action_effects::ActionEffects;

/// One action done: the verb as a result line starts with it (`click`),
/// what was acted on and read back (`"Price ($)" "67.00" -> "62.00"`), the
/// `--json` data, what the action set off when it was watched, and the
/// element tokens the agent's view already held.
#[derive(Debug, Clone)]
pub(crate) struct Acted {
    pub(crate) verb: &'static str,
    pub(crate) summary: String,
    pub(crate) data: Value,
    pub(crate) effects: Option<ActionEffects>,
    pub(crate) known: HashSet<String>,
}
