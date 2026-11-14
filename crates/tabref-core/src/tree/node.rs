use serde::{Deserialize, Serialize};

use super::node_identity::NodeIdentity;
use super::node_presentation::NodePresentation;

/// One node of a page's accessibility tree in tabref's role vocabulary.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AccessibilityNode {
    #[serde(skip_serializing_if = "Option::is_none", rename = "ref")]
    pub ref_id: Option<String>,
    pub role: String,
    #[serde(flatten)]
    pub identity: NodeIdentity,
    #[serde(flatten)]
    pub presentation: NodePresentation,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<AccessibilityNode>,
}

impl AccessibilityNode {
    /// The name to show: the accessible name, or else the derived one.
    pub fn display_name(&self) -> Option<&str> {
        self.identity
            .name
            .as_deref()
            .or(self.presentation.derived_name.as_deref())
            .filter(|name| !name.trim().is_empty())
    }

    /// The name a ref records as identity. A derived name never counts.
    pub fn identity_name(&self) -> Option<&str> {
        match self.presentation.name_source {
            Some(_) => None,
            None => self
                .identity
                .name
                .as_deref()
                .filter(|name| !name.is_empty()),
        }
    }
}
