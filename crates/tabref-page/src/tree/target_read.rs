use serde_json::Value;
use tabref_core::AppError;

use super::cdp_ax_node::CdpAxNode;
use tabref_cdp::transport_failed;

/// The nodes of an `Accessibility.getFullAXTree` reply.
pub(crate) fn parse_nodes(mut reply: Value) -> Result<Vec<CdpAxNode>, AppError> {
    let nodes = reply.get_mut("nodes").map_or(Value::Null, Value::take);
    serde_json::from_value(nodes)
        .map_err(|error| transport_failed("Accessibility.getFullAXTree", error.to_string()))
}
