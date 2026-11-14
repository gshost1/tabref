use serde_json::json;

use crate::output::outcome::Outcome;

const SKILL: &str = include_str!("../skill/SKILL.md");

/// Prints the agent guide compiled into the binary.
pub(crate) fn run() -> Outcome {
    Outcome::new(SKILL.trim_end(), json!({ "skill": SKILL }))
}
