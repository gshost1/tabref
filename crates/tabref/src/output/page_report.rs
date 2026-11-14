use serde_json::json;
use tabref_core::AccessibilityNode;

use super::outcome::Outcome;
use crate::session::captured_snapshot::CapturedSnapshot;
use crate::view::node_line::quote_text;
use crate::view::tree_lines::tree_lines;
use crate::view::view_mode::ViewMode;

/// A snapshot as a command prints it: a `page "Title" url` header with the
/// ref count, one line per shown node, and a note when frames were left out.
pub(crate) fn page_report(captured: &CapturedSnapshot, mode: ViewMode) -> Outcome {
    subtree_report(captured, &captured.tree, mode)
}

/// [`page_report`] of one subtree of the snapshot.
pub(crate) fn subtree_report(
    captured: &CapturedSnapshot,
    root: &AccessibilityNode,
    mode: ViewMode,
) -> Outcome {
    let lines = tree_lines(root, mode);
    let mut text = format!(
        "page {} {} ({} refs)",
        quote_text(&captured.info.title),
        captured.info.url,
        captured.ref_count
    );
    for line in &lines {
        text.push('\n');
        text.push_str(line);
    }
    if captured.unreadable_frames > 0 {
        text.push_str(&format!(
            "\nnote: {} frame(s) could not be read in time and are left out",
            captured.unreadable_frames
        ));
    }
    Outcome::new(
        text,
        json!({
            "snapshot_id": captured.snapshot_id,
            "url": captured.info.url,
            "title": captured.info.title,
            "ref_count": captured.ref_count,
            "unreadable_frames": captured.unreadable_frames,
            "lines": lines,
        }),
    )
}
