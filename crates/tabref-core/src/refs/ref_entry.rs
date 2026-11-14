use serde::{Deserialize, Serialize};

/// What a snapshot remembers about one ref: the element token that
/// addresses its DOM node, and the role and accessible name it had, which an
/// action compares against the live node so a ref never silently reaches a
/// different element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefEntry {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub token: String,
}
