use serde::Deserialize;

use super::cdp_ax_node::CdpAxNode;

/// The `result` of an `Accessibility.getFullAXTree` reply.
#[derive(Debug, Deserialize)]
pub(crate) struct AxNodes {
    #[serde(default)]
    pub(crate) nodes: Vec<CdpAxNode>,
}
