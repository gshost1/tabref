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

/// Scripts the tab answering that it shows, at device scale `scale`.
fn shown(transport: ScriptedTransport, scale: f64) -> ScriptedTransport {
    transport.expect(
        "Runtime.evaluate",
        json!({ "result": { "type": "object", "value": ["visible", scale] } }),
    )
}

/// Scripts the reads an element clip makes, for a border box `quad` in a
/// page scrolled down by `page_y` with an 800 by 600 viewport.
fn element(
    transport: ScriptedTransport,
    quad: serde_json::Value,
    page_y: f64,
) -> ScriptedTransport {
    transport
        .expect(
            "Page.getFrameTree",
            json!({ "frameTree": { "frame": { "id": "F", "loaderId": "L" } } }),
        )
        .expect("DOM.scrollIntoViewIfNeeded", json!({}))
        .expect("DOM.getBoxModel", json!({ "model": { "border": quad } }))
        .expect("Page.getLayoutMetrics", metrics(page_y))
        .expect("Page.captureScreenshot", json!({ "data": "iVBORw0KGgo=" }))
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
fn the_viewport_is_captured_as_png_without_a_clip_with_the_device_scale() {
    let data = "iVBORw0KGgo=";
    let transport = shown(ScriptedTransport::new(), 2.0)
        .expect("Page.captureScreenshot", json!({ "data": data }));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let image = capture_screenshot(&mut connection, &session(), None, deadline()).unwrap();

    assert_eq!(image.png, decode_base64(data).unwrap());
    assert_eq!(image.device_scale, 2.0);
    let sent = sent.lock().unwrap();
    assert_eq!(sent[1]["params"]["format"], "png");
    assert!(sent[1]["params"].get("clip").is_none(), "{}", sent[1]);
}

#[test]
fn a_tab_that_is_not_showing_is_refused_before_anything_is_captured() {
    let transport = ScriptedTransport::new().expect(
        "Runtime.evaluate",
        json!({ "result": { "type": "object", "value": ["hidden", 1] } }),
    );
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let error = capture_screenshot(&mut connection, &session(), None, deadline()).unwrap_err();

    assert_eq!(error.kind(), Some("screenshot_tab_hidden"), "{error:?}");
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[test]
fn an_element_is_clipped_where_it_sits_in_the_document_after_scrolling_to_it() {
    let quad = json!([10, 20, 110, 20, 110, 70, 10, 70]);
    let transport = element(shown(ScriptedTransport::new(), 1.0), quad, 1480.0);
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    capture_screenshot(&mut connection, &session(), Some(&token(9)), deadline()).unwrap();

    let sent = sent.lock().unwrap();
    let params = &sent[5]["params"];
    let clip = &params["clip"];
    assert_eq!(clip["x"], 10.0, "{clip}");
    assert_eq!(clip["y"], 1500.0, "{clip}");
    assert_eq!(clip["width"], 100.0, "{clip}");
    assert_eq!(clip["height"], 50.0, "{clip}");
    assert_eq!(clip["scale"], 1.0, "{clip}");
    assert_ne!(params["captureBeyondViewport"], true, "{params}");
}

#[test]
fn an_element_taller_than_the_viewport_is_captured_beyond_it() {
    let quad = json!([10, 0, 130, 0, 130, 2000, 10, 2000]);
    let transport = element(shown(ScriptedTransport::new(), 1.0), quad, 300.0);
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    capture_screenshot(&mut connection, &session(), Some(&token(9)), deadline()).unwrap();

    let sent = sent.lock().unwrap();
    let params = &sent[5]["params"];
    assert_eq!(params["clip"]["height"], 2000.0, "{params}");
    assert_eq!(params["captureBeyondViewport"], true, "{params}");
}

#[test]
fn an_element_inside_a_frame_is_refused_before_anything_is_captured() {
    let transport = shown(ScriptedTransport::new(), 1.0).expect(
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
    assert_eq!(sent.lock().unwrap().len(), 2);
}

#[test]
fn an_element_of_a_replaced_document_is_a_stale_ref() {
    let transport = shown(ScriptedTransport::new(), 1.0).expect(
        "Page.getFrameTree",
        json!({ "frameTree": { "frame": { "id": "F", "loaderId": "NEWER" } } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let error =
        capture_screenshot(&mut connection, &session(), Some(&token(9)), deadline()).unwrap_err();

    assert_eq!(error.code, tabref_core::ErrorCode::StaleRef, "{error:?}");
}
