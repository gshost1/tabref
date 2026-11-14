use serde_json::json;

use super::{OwnerBox, collect_owner_boxes};
use crate::tree::frame_skip_reason::FrameSkipReason;

fn owner(x: f64, y: f64, width: f64, height: f64) -> OwnerBox {
    OwnerBox {
        x,
        y,
        width,
        height,
        visible: true,
    }
}

#[test]
fn boxes_that_cannot_show_a_frame_name_their_reason() {
    assert_eq!(owner(10.0, 10.0, 300.0, 250.0).skip_reason(), None);
    assert_eq!(
        owner(10.0, 10.0, 0.0, 250.0).skip_reason(),
        Some(FrameSkipReason::ZeroSize)
    );
    assert_eq!(
        owner(10.0, 10.0, 1.0, 1.0).skip_reason(),
        Some(FrameSkipReason::Tiny)
    );
    assert_eq!(
        owner(-9_999.0, 10.0, 300.0, 250.0).skip_reason(),
        Some(FrameSkipReason::Offscreen)
    );
    let hidden = OwnerBox {
        visible: false,
        ..owner(0.0, 0.0, 300.0, 250.0)
    };
    assert_eq!(hidden.skip_reason(), Some(FrameSkipReason::Hidden));
    assert_eq!(owner(0.0, 4_000.0, 300.0, 250.0).skip_reason(), None);
}

#[test]
fn only_frame_elements_are_collected_with_their_visibility() {
    let capture = json!({
        "strings": ["IFRAME", "DIV", "block", "visible", "hidden", "iframe"],
        "documents": [{
            "nodes": { "nodeName": [1, 0, 5], "backendNodeId": [7, 8, 9] },
            "layout": {
                "nodeIndex": [0, 1, 2],
                "bounds": [[0, 0, 800, 600], [5, 5, 1, 1], [0, 0, 300, 200]],
                "styles": [[2, 3], [2, 3], [2, 4]],
            },
        }],
    });

    let boxes = collect_owner_boxes(&capture).unwrap();

    assert_eq!(boxes.len(), 2);
    assert_eq!(boxes[&8].skip_reason(), Some(FrameSkipReason::Tiny));
    assert_eq!(boxes[&9].skip_reason(), Some(FrameSkipReason::Hidden));
}

#[test]
fn a_capture_without_layout_says_nothing() {
    let capture = json!({ "strings": [], "documents": [{ "nodes": {} }] });
    assert!(collect_owner_boxes(&capture).is_none());
}
