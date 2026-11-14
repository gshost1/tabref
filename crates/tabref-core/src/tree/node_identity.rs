use serde::{Deserialize, Serialize};

/// What names a node. `retained_object` is the element token a ref carries
/// back to the browser; it never prints.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NodeIdentity {
    #[serde(skip)]
    pub retained_object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom_id: Option<String>,
}
