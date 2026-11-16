use serde_json::json;

use crate::output::outcome::Outcome;

const SKILL: &str = include_str!("../skill/SKILL.md");
const SKILL_MORE: &str = include_str!("../skill/SKILL_MORE.md");

/// Prints the agent guide compiled into the binary; `more` prints the
/// rare commands kept out of it so the guide stays short.
pub(crate) fn run(topic: Option<&str>) -> Outcome {
    let text = match topic {
        Some("more") => SKILL_MORE,
        _ => SKILL,
    };
    Outcome::new(text.trim_end(), json!({ "skill": text }))
}

#[cfg(test)]
#[path = "skill_tests.rs"]
mod tests;
