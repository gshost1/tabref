use std::fmt;

use super::snapshot_id::validate_snapshot_id;
use crate::AppError;

/// A snapshot-qualified ref, `@<snapshot_id>:e<N>`, as `snap` prints it. The
/// snapshot id names the refmap the element number belongs to, so a ref from
/// an older snapshot is looked up there and never in a newer one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RefId {
    pub snapshot_id: String,
    pub element: u32,
}

impl RefId {
    pub fn new(snapshot_id: impl Into<String>, element: u32) -> Self {
        Self {
            snapshot_id: snapshot_id.into(),
            element,
        }
    }

    /// Parses `@s8f3k2p9:e4`. The leading `@` may be left out; a bare `@e4`
    /// names no snapshot and is refused.
    pub fn parse(text: &str) -> Result<Self, AppError> {
        let trimmed = text.trim();
        let without_at = trimmed.strip_prefix('@').unwrap_or(trimmed);
        let (snapshot_id, element) = without_at.rsplit_once(":e").ok_or_else(|| invalid(text))?;
        validate_snapshot_id(snapshot_id).map_err(|_| invalid(text))?;
        let element = parse_element(element).ok_or_else(|| invalid(text))?;
        Ok(Self::new(snapshot_id, element))
    }

    /// Whether `text` looks like a ref rather than ordinary text.
    pub fn looks_like_ref(text: &str) -> bool {
        Self::parse(text).is_ok()
    }
}

fn parse_element(digits: &str) -> Option<u32> {
    if digits.is_empty() || digits.len() > 10 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok().filter(|number| *number > 0)
}

fn invalid(text: &str) -> AppError {
    AppError::invalid_args(format!("'{text}' is not a ref: expected @<snapshot>:e<N>"))
        .with_suggestion("Copy the ref from the start of a 'tabref snap' line, e.g. @s8f3k2p9:e4")
        .with_details(serde_json::json!({ "kind": "ref_malformed" }))
}

impl fmt::Display for RefId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "@{}:e{}", self.snapshot_id, self.element)
    }
}

#[cfg(test)]
#[path = "ref_id_tests.rs"]
mod tests;
