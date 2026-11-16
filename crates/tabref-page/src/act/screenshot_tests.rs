use serde_json::json;

use super::{decode_base64, png_size};
use crate::act::act_fixture::{deadline, metrics, session, token};
use crate::{CdpConnection, capture_screenshot};
use tabref_cdp::ScriptedTransport;

/// A PNG signature and IHDR chunk for a `width` by `height` image.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes
}

#[test]
fn base64_decodes_with_and_without_padding_and_refuses_other_text() {
    assert_eq!(decode_base64("TWFu").unwrap(), b"Man");
    assert_eq!(decode_base64("TWE=").unwrap(), b"Ma");
    assert_eq!(decode_base64("TQ==").unwrap(), b"M");
    assert_eq!(decode_base64("").unwrap(), b"");
    assert_eq!(decode_base64("+/8=").unwrap(), [0xfb, 0xff]);
    assert!(decode_base64("TW!u").is_err());
    assert!(decode_base64("TWF").is_err());
}

#[test]
fn the_pixel_size_is_read_from_the_png_header() {
    assert_eq!(png_size(&png_header(800, 600)), Some((800, 600)));
    assert_eq!(png_size(b"GIF89a not a png at all......"), None);
    assert_eq!(png_size(&png_header(1, 1)[..20]), None);
}

#[test]
fn the_viewport_is_captured_as_png_without_a_clip() {
    let data = "iVBORw0KGgo=";
    let transport =
        ScriptedTransport::new().expect("Page.captureScreenshot", json!({ "data": data }));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let png = capture_screenshot(&mut connection, &session(), None, deadline()).unwrap();

    assert_eq!(png, decode_base64(data).unwrap());
    let sent = sent.lock().unwrap();
    assert_eq!(sent[0]["params"]["format"], "png");
    assert!(sent[0]["params"].get("clip").is_none(), "{}", sent[0]);
}

#[test]
fn an_element_is_clipped_where_it_sits_in_the_document_after_scrolling_to_it() {
    let transport = ScriptedTransport::new()
        .expect(
            "Page.getFrameTree",
            json!({ "frameTree": { "frame": { "id": "F", "loaderId": "L" } } }),
        )
        .expect("DOM.scrollIntoViewIfNeeded", json!({}))
        .expect(
            "DOM.getBoxModel",
            json!({ "model": { "border": [10, 20, 110, 20, 110, 70, 10, 70] } }),
        )
        .expect("Page.getLayoutMetrics", metrics(1480.0))
        .expect("Page.captureScreenshot", json!({ "data": "iVBORw0KGgo=" }));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    capture_screenshot(&mut connection, &session(), Some(&token(9)), deadline()).unwrap();

    let sent = sent.lock().unwrap();
    let clip = &sent[4]["params"]["clip"];
    assert_eq!(clip["x"], 10.0, "{clip}");
    assert_eq!(clip["y"], 1500.0, "{clip}");
    assert_eq!(clip["width"], 100.0, "{clip}");
    assert_eq!(clip["height"], 50.0, "{clip}");
    assert_eq!(clip["scale"], 1.0, "{clip}");
}

#[test]
fn an_element_inside_a_frame_is_refused_before_anything_is_captured() {
    let transport = ScriptedTransport::new().expect(
        "Page.getFrameTree",
        json!({ "frameTree": {
            "frame": { "id": "MAIN", "loaderId": "L0" },
            "childFrames": [ { "frame": { "id": "F", "loaderId": "L" } } ]
        } }),
    );
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let error =
        capture_screenshot(&mut connection, &session(), Some(&token(9)), deadline()).unwrap_err();

    assert_eq!(
        error.kind(),
        Some("screenshot_frame_unsupported"),
        "{error:?}"
    );
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[test]
fn an_element_of_a_replaced_document_is_a_stale_ref() {
    let transport = ScriptedTransport::new().expect(
        "Page.getFrameTree",
        json!({ "frameTree": { "frame": { "id": "F", "loaderId": "NEWER" } } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let error =
        capture_screenshot(&mut connection, &session(), Some(&token(9)), deadline()).unwrap_err();

    assert_eq!(error.code, tabref_core::ErrorCode::StaleRef, "{error:?}");
}
