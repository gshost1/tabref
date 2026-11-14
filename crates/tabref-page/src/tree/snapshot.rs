use serde_json::json;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::flatten::flatten;
use super::frame_budget::FrameBudget;
use super::frame_graft::graft_frames;
use super::frame_reader::FrameReader;
use super::frame_skip_reason::FrameSkipReason;
use super::skipped_frame::SkippedFrame;
use crate::{CdpConnection, PageSession, WebSnapshot};

/// Reads the full accessibility tree of every frame in the page, including
/// out-of-process iframes, which are attached as their own targets and read
/// in parallel, grafts each child frame beneath its owner element, and
/// returns it in core form. Frames whose owner element cannot show anything
/// (hidden, no area, a pixel, off the page), frames that hold no text or
/// control, and frames that cannot be read are left out and listed in
/// `skipped_frames`. Every iframe is read within `deadline`, and one that
/// misses it fails the snapshot.
pub fn snapshot_page(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<WebSnapshot, AppError> {
    snapshot_with(connection, session, FrameBudget::whole(deadline))
}

/// Reads the page as [`snapshot_page`] does, but waits for iframes only a
/// short while after the page itself is read (see [`FrameBudget`]), so a
/// slow or stalled iframe cannot cost the page. An iframe that misses the
/// budget is listed as timed out.
pub fn snapshot_page_within_budget(
    connection: &mut CdpConnection,
    session: &PageSession,
    deadline: Deadline,
) -> Result<WebSnapshot, AppError> {
    snapshot_with(connection, session, FrameBudget::reserved(deadline))
}

fn snapshot_with(
    connection: &mut CdpConnection,
    session: &PageSession,
    budget: FrameBudget,
) -> Result<WebSnapshot, AppError> {
    let mut reader = FrameReader::new(session, budget);
    let read = reader.run(connection);
    reader.release(connection);
    read?;
    let frames = reader.assemble();
    let (roots, dropped) = graft_frames(frames);
    for frame_id in dropped {
        reader.leave_out(&frame_id, String::new(), FrameSkipReason::Unreadable);
    }
    let mut roots = roots.into_iter();
    let (Some(root), None) = (roots.next(), roots.next()) else {
        return Err(no_tree(&session.target_id));
    };
    let mut origins = Vec::new();
    let tree = flatten(root, &mut origins);
    let skipped_frames: Vec<SkippedFrame> = reader.left_out;
    Ok(WebSnapshot {
        tree,
        origins,
        unreadable_frames: skipped_frames
            .iter()
            .filter(|frame| frame.reason.is_unread())
            .map(|frame| frame.frame_id.clone())
            .collect(),
        skipped_frames,
    })
}

fn no_tree(target_id: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("Page target {target_id} exposed no single accessibility tree root"),
    )
    .with_details(json!({ "kind": "cdp_tree_empty", "target_id": target_id }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
