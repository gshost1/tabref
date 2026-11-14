use serde::{Deserialize, Serialize};

use super::name_source::NameSource;

/// How a node reads and what it can do: its state tokens, the actions it
/// advertises, and a name derived from its text when it has no accessible
/// one. `derived_name` never enters ref identity.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodePresentation {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub states: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub available_actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name_source: Option<NameSource>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub derived_name: Option<String>,
}
