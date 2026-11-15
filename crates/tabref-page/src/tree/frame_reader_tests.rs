use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tabref_core::{Deadline, ErrorCode};

use crate::CdpConnection;
use crate::tree::frame_skip_reason::FrameSkipReason;
use crate::tree::snapshot::{snapshot_page, snapshot_page_within_budget};
use crate::tree::snapshot_fixture::{
    attached, capture, capture_with_owner, child_reads, child_tree, frame_tree, main_frame_only,
    page_reads, release, session, stalled_child,
};

fn deadline() -> Deadline {
    Deadline::after(1_000).unwrap()
}

#[test]
fn a_stalled_iframe_is_left_out_after_the_budget_and_its_sibling_still_read() {
    let children = vec![attached("S2", "SLOW"), attached("S3", "FAST")];
    let transport = stalled_child(page_reads(children, main_frame_only(), json!({})));
    let transport = release(child_reads(transport, "FAST"), 3);
    let mut connection = CdpConnection::new(transport.boxed());
    let started = Instant::now();

    let snapshot = snapshot_page_within_budget(
        &mut connection,
        &session(),
        Deadline::after(10_000).unwrap(),
    )
    .unwrap();

    assert!(
        started.elapsed() < Duration::from_millis(900),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(snapshot.unreadable_frames, ["SLOW"]);
    assert_eq!(snapshot.skipped_frames[0].reason, FrameSkipReason::TimedOut);
    assert_eq!(
        snapshot.preorder()[3].0.identity.name.as_deref(),
        Some("Inner")
    );
}

#[test]
fn a_pixel_sized_iframe_is_left_out_as_soon_as_its_owner_box_is_known() {
    let transport = page_reads(
        vec![attached("S2", "AD")],
        main_frame_only(),
        capture_with_owner([10.0, 10.0, 1.0, 1.0]),
    );
    let mut connection = CdpConnection::new(release(stalled_child(transport), 2).boxed());
    let started = Instant::now();

    let snapshot = snapshot_page_within_budget(
        &mut connection,
        &session(),
        Deadline::after(10_000).unwrap(),
    )
    .unwrap();

    assert!(
        started.elapsed() < Duration::from_millis(200),
        "{:?}",
        started.elapsed()
    );
    assert!(snapshot.unreadable_frames.is_empty());
    assert_eq!(snapshot.skipped_frames.len(), 1);
    assert_eq!(snapshot.skipped_frames[0].reason, FrameSkipReason::Tiny);
    assert_eq!(snapshot.skipped_frames[0].url, "https://AD.test/");
}

#[test]
fn a_visible_iframe_box_keeps_the_frame() {
    let transport = page_reads(
        vec![attached("S2", "OOP")],
        main_frame_only(),
        capture_with_owner([10.0, 10.0, 300.0, 250.0]),
    );
    let mut connection = CdpConnection::new(release(child_reads(transport, "OOP"), 2).boxed());

    let snapshot = snapshot_page(&mut connection, &session(), deadline()).unwrap();

    assert!(snapshot.skipped_frames.is_empty());
    assert_eq!(snapshot.preorder().len(), 5);
}

#[test]
fn iframes_past_the_budget_are_listed_without_being_asked() {
    let transport = page_reads(vec![attached("S2", "OOP")], main_frame_only(), json!({}));
    let transport = release(transport, 1);
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    let snapshot =
        snapshot_page_within_budget(&mut connection, &session(), Deadline::after(50).unwrap())
            .unwrap();

    assert_eq!(snapshot.unreadable_frames, ["OOP"]);
    assert_eq!(snapshot.preorder().len(), 3);
    let sent = sent.lock().unwrap();
    assert!(sent.iter().all(|request| request["sessionId"] != "S2"));
}

#[test]
fn an_iframe_whose_child_frames_cannot_be_listed_in_time_is_kept() {
    let frames = json!({ "frameTree": {
        "frame": { "id": "OOP", "parentId": "MAIN", "loaderId": "LO" },
    } });
    let transport = page_reads(vec![attached("S2", "OOP")], main_frame_only(), json!({}))
        .expect_silence("Target.setAutoAttach")
        .expect("Page.getFrameTree", frames)
        .expect("DOMSnapshot.captureSnapshot", json!({}))
        .expect("Accessibility.getFullAXTree", child_tree())
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }));
    let mut connection = CdpConnection::new(release(transport, 2).boxed());

    let snapshot = snapshot_page_within_budget(&mut connection, &session(), deadline()).unwrap();

    assert_eq!(snapshot.unreadable_frames, ["OOP"]);
    assert_eq!(snapshot.preorder().len(), 5);
}

#[test]
fn an_iframe_whose_dom_capture_misses_the_budget_is_left_out_as_timed_out() {
    let frames = json!({ "frameTree": {
        "frame": { "id": "OOP", "parentId": "MAIN", "loaderId": "LO" },
    } });
    let transport = page_reads(vec![attached("S2", "OOP")], main_frame_only(), json!({}))
        .expect("Target.setAutoAttach", json!({}))
        .expect("Page.getFrameTree", frames)
        .expect_silence("DOMSnapshot.captureSnapshot")
        .expect("Accessibility.getFullAXTree", child_tree())
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }));
    let mut connection = CdpConnection::new(release(transport, 2).boxed());

    let snapshot = snapshot_page_within_budget(&mut connection, &session(), deadline()).unwrap();

    assert_eq!(snapshot.unreadable_frames, ["OOP"]);
    assert_eq!(snapshot.preorder().len(), 3);
}

#[test]
fn a_whole_deadline_snapshot_still_fails_on_a_slow_iframe() {
    let transport = page_reads(vec![attached("S2", "SLOW")], main_frame_only(), json!({}));
    let mut connection = CdpConnection::new(release(stalled_child(transport), 2).boxed());

    let error =
        snapshot_page(&mut connection, &session(), Deadline::after(300).unwrap()).unwrap_err();

    assert_eq!(error.code, ErrorCode::Timeout);
}

fn in_page_frames(child_ax: Option<Value>) -> tabref_cdp::ScriptedTransport {
    let transport = page_reads(Vec::new(), frame_tree(), capture());
    let transport = match child_ax {
        Some(tree) => transport.expect("Accessibility.getFullAXTree", tree),
        None => transport.expect_silence("Accessibility.getFullAXTree"),
    };
    transport
        .expect("DOM.getFrameOwner", json!({ "backendNodeId": 30 }))
        .expect("Accessibility.getFullAXTree", child_tree())
        .expect_silence("DOM.getFrameOwner")
}

#[test]
fn an_in_page_iframe_that_misses_its_budget_is_listed_and_the_page_returned() {
    let mut connection = CdpConnection::new(release(in_page_frames(None), 1).boxed());

    let snapshot = snapshot_page_within_budget(&mut connection, &session(), deadline()).unwrap();

    assert_eq!(snapshot.unreadable_frames, ["CHILD"]);
    assert_eq!(snapshot.preorder().len(), 5);
}

#[test]
fn a_whole_deadline_snapshot_still_fails_on_a_slow_in_page_iframe() {
    let mut connection = CdpConnection::new(release(in_page_frames(None), 1).boxed());

    let error =
        snapshot_page(&mut connection, &session(), Deadline::after(300).unwrap()).unwrap_err();

    assert_eq!(error.code, ErrorCode::Timeout);
}
