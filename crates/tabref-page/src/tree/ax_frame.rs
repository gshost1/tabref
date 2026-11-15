use serde::Deserialize;

use super::ax_nodes::AxNodes;

/// A whole `Accessibility.getFullAXTree` reply frame, read straight from its
/// text into nodes; its id and session are skipped.
#[derive(Debug, Deserialize)]
pub(crate) struct AxFrame {
    pub(crate) result: AxNodes,
}
