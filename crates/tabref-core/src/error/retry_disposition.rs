use serde::{Deserialize, Serialize};

/// Whether repeating the command cannot deliver an action twice.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RetryDisposition {
    #[default]
    Unknown,
    Safe,
    Unsafe,
}
