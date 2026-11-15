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
fn any_owner_element_is_judged_by_its_box_and_an_absent_one_is_kept() {
    let capture = json!({
        "strings": ["HTML", "OBJECT", "EMBED", "block", "visible", "hidden", "FENCEDFRAME"],
        "documents": [{
            "nodes": { "nodeName": [0, 1, 2, 6, 1], "backendNodeId": [7, 8, 9, 10, 11] },
            "layout": {
                "nodeIndex": [0, 1, 2, 3],
                "bounds": [[0, 0, 800, 600], [10, 10, 300, 200], [5, 5, 1, 1], [0, 0, 300, 200]],
                "styles": [[3, 4], [3, 4], [3, 4], [3, 5]],
            },
        }],
    });

    let owners = collect_owner_boxes(&capture).unwrap();

    assert_eq!(owners.skip_reason(8), None);
    assert_eq!(owners.skip_reason(9), Some(FrameSkipReason::Tiny));
    assert_eq!(owners.skip_reason(10), Some(FrameSkipReason::Hidden));
    assert_eq!(owners.skip_reason(11), Some(FrameSkipReason::Hidden));
    assert_eq!(owners.skip_reason(999), None);
}

#[test]
fn a_capture_without_layout_says_nothing() {
    let capture = json!({ "strings": [], "documents": [{ "nodes": {} }] });
    assert!(collect_owner_boxes(&capture).is_none());
}
