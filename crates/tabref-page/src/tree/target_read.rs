use serde_json::Value;
use tabref_core::AppError;

use super::ax_frame::AxFrame;
use super::cdp_ax_node::CdpAxNode;
use tabref_cdp::transport_failed;

/// The nodes of an `Accessibility.getFullAXTree` reply: either its parsed
/// result, or, when it was asked for raw, the whole reply frame's text,
/// which is read straight into nodes without an intermediate JSON value.
pub(crate) fn parse_nodes(mut reply: Value) -> Result<Vec<CdpAxNode>, AppError> {
    let parsed = match &reply {
        Value::String(frame) => {
            serde_json::from_str::<AxFrame>(frame).map(|frame| frame.result.nodes)
        }
        _ => serde_json::from_value(reply.get_mut("nodes").map_or(Value::Null, Value::take)),
    };
    parsed.map_err(|error| transport_failed("Accessibility.getFullAXTree", error.to_string()))
}
