use serde_json::{Value, json};
use tabref_cdp::{PageSession, list_frames};
use tabref_core::{AppError, Deadline, ErrorCode};
use tabref_page::ElementToken;

use crate::session::Session;

/// Resolves one file input in the current tab's main document, including
/// hidden inputs. Explicit CSS never falls back to a nearby input.
pub(super) fn resolve(
    session: &mut Session,
    selector: &str,
    deadline: Deadline,
) -> Result<(PageSession, ElementToken), AppError> {
    let page = session.page()?;
    let connection = session.connection();
    let frame = list_frames(connection, &page, deadline)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("The current page has no main frame"))?;
    let address = Some(page.session_id.as_str());
    let document = connection.call("DOM.getDocument", json!({ "depth": 0 }), address, deadline)?;
    let root = document
        .pointer("/root/nodeId")
        .and_then(Value::as_i64)
        .ok_or_else(|| AppError::internal("The current page has no document node"))?;
    let matches = connection.call(
        "DOM.querySelectorAll",
        json!({ "nodeId": root, "selector": selector }),
        address,
        deadline,
    )?;
    let nodes = matches["nodeIds"]
        .as_array()
        .ok_or_else(|| AppError::internal("The selector returned no node list"))?;
    let node_id = match nodes.as_slice() {
        [node] => node,
        [] => {
            return Err(AppError::new(
                ErrorCode::ElementNotFound,
                "CSS matched no element",
            ));
        }
        _ => {
            return Err(AppError::new(
                ErrorCode::AmbiguousTarget,
                "CSS matched several elements",
            ));
        }
    };
    let described = connection.call(
        "DOM.describeNode",
        json!({ "nodeId": node_id }),
        address,
        deadline,
    )?;
    let node = &described["node"];
    let is_file = node["localName"].as_str() == Some("input")
        && node["attributes"].as_array().is_some_and(|attributes| {
            attributes.chunks_exact(2).any(|pair| {
                pair[0].as_str() == Some("type")
                    && pair[1]
                        .as_str()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("file"))
            })
        });
    if !is_file {
        return Err(AppError::invalid_args("CSS must target a file input"));
    }
    let token = ElementToken {
        target_id: page.target_id.clone(),
        frame_id: frame.frame_id,
        loader_id: frame.loader_id,
        backend_node_id: node["backendNodeId"]
            .as_i64()
            .ok_or_else(|| AppError::internal("The selected file input has no backend node ID"))?,
    };
    Ok((page, token))
}
