use serde::{Deserialize, Serialize};

/// Where a node's displayed name came from when no accessible name supplied
/// it. A derived name is shown and searched, but a ref never records it, so
/// it can never prove or re-identify a ref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameSource {
    /// The text a web element contains, for an element that exposes no
    /// accessible name: a clickable `<div>` row, or a block of text runs.
    ContainedText,
}
