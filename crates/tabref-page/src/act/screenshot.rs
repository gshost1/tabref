//! A PNG of the tab (`Page.captureScreenshot`): the visible part of the
//! page, or one element clipped where it sits in the document. The element
//! is scrolled into view first, and only an element of the tab's own main
//! document is taken: the clip is in the main frame's coordinates, and an
//! element inside a frame is refused rather than clipped at the wrong place.

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::element_errors::{document_replaced, wrong_session};
use super::scroll::{not_rendered, not_rendered_error, scroll_node};
use super::viewport_metrics::ViewportMetrics;
use crate::{CdpConnection, ElementToken, PageSession};

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// The tab's visible area as PNG bytes, or the element `element` names,
/// clipped to its border box.
pub fn capture_screenshot(
    connection: &mut CdpConnection,
    session: &PageSession,
    element: Option<&ElementToken>,
    deadline: Deadline,
) -> Result<Vec<u8>, AppError> {
    let mut params = json!({ "format": "png" });
    if let Some(token) = element {
        params["clip"] = element_clip(connection, session, token, deadline)?;
    }
    let reply = connection.call(
        "Page.captureScreenshot",
        params,
        Some(&session.session_id),
        deadline,
    )?;
    let data = reply
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::internal("Page.captureScreenshot returned no image"))?;
    decode_base64(data)
}

/// The width and height a PNG's header gives, or `None` for other bytes.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if !png.starts_with(PNG_SIGNATURE) || png.get(12..16)? != b"IHDR" {
        return None;
    }
    let number = |at: usize| Some(u32::from_be_bytes(png.get(at..at + 4)?.try_into().ok()?));
    Some((number(16)?, number(20)?))
}

fn element_clip(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Value, AppError> {
    if session.target_id != token.target_id {
        return Err(wrong_session(token, &session.target_id));
    }
    let tree = connection.call(
        "Page.getFrameTree",
        json!({}),
        Some(&session.session_id),
        deadline,
    )?;
    let main = &tree["frameTree"]["frame"];
    if main["id"].as_str() != Some(token.frame_id.as_str()) {
        return Err(in_frame(token));
    }
    let loader = main["loaderId"].as_str();
    if loader != Some(token.loader_id.as_str()) {
        return Err(document_replaced(token, loader));
    }
    scroll_node(connection, session, token, deadline)?;
    let reply = connection
        .call(
            "DOM.getBoxModel",
            json!({ "backendNodeId": token.backend_node_id }),
            Some(&session.session_id),
            deadline,
        )
        .map_err(|error| not_rendered(error, token))?;
    let (x, y, width, height) = bounds(reply.pointer("/model/border"))
        .ok_or_else(|| not_rendered_error(token.backend_node_id, "the node has an empty box"))?;
    let viewport = ViewportMetrics::read(connection, session, deadline)?;
    Ok(json!({
        "x": viewport.page_x + x / viewport.zoom,
        "y": viewport.page_y + y / viewport.zoom,
        "width": width / viewport.zoom,
        "height": height / viewport.zoom,
        "scale": 1.0,
    }))
}

/// The left, top, width and height of a quad's bounding box, when it has
/// an area.
fn bounds(quad: Option<&Value>) -> Option<(f64, f64, f64, f64)> {
    let numbers: Vec<f64> = quad?.as_array()?.iter().filter_map(Value::as_f64).collect();
    if numbers.len() != 8 {
        return None;
    }
    let xs = [numbers[0], numbers[2], numbers[4], numbers[6]];
    let ys = [numbers[1], numbers[3], numbers[5], numbers[7]];
    let left = xs.iter().copied().fold(f64::MAX, f64::min);
    let top = ys.iter().copied().fold(f64::MAX, f64::min);
    let width = xs.iter().copied().fold(f64::MIN, f64::max) - left;
    let height = ys.iter().copied().fold(f64::MIN, f64::max) - top;
    (width > 0.0 && height > 0.0).then_some((left, top, width, height))
}

fn in_frame(token: &ElementToken) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        "tabref screenshots an element only in the page's main document, and this one is \
         inside a frame",
    )
    .with_suggestion("Run 'tabref screenshot' without a target for the whole visible page")
    .with_details(json!({
        "kind": "screenshot_frame_unsupported",
        "frame_id": token.frame_id,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// Decodes standard base64 (`A-Z a-z 0-9 + /`, `=` padding), as
/// `Page.captureScreenshot` sends its image.
pub(crate) fn decode_base64(text: &str) -> Result<Vec<u8>, AppError> {
    let invalid = || AppError::internal("Page.captureScreenshot sent an image that is not base64");
    let bytes = text.trim_end_matches('=').as_bytes();
    if text.len() % 4 != 0 || text.len() - bytes.len() > 2 {
        return Err(invalid());
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buffer = 0_u32;
    let mut bits = 0;
    for &byte in bytes {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err(invalid()),
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "screenshot_tests.rs"]
mod tests;
