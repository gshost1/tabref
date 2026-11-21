use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;
use tabref_core::AccessibilityNode;
use tabref_page::{DomTree, PageInfo, SkippedFrame, parse_capture};

/// A snapshot read and saved: its id, the tree with refs allocated, the
/// page's address and title, how many refs it holds, how many frames could
/// not be read, and every frame left out of the tree with its reason (ads
/// and trackers that can show nothing, empty frames, timed-out frames), for
/// the view to fold into one line, and the DOM capture the snapshot took of
/// each target it kept, by target id, which the page view reads instead of
/// capturing again.
#[derive(Debug, Clone)]
pub(crate) struct CapturedSnapshot {
    pub(crate) snapshot_id: String,
    pub(crate) tree: AccessibilityNode,
    pub(crate) info: PageInfo,
    pub(crate) ref_count: usize,
    pub(crate) unreadable_frames: usize,
    pub(crate) skipped_frames: Vec<SkippedFrame>,
    pub(super) dom_captures: HashMap<String, CapturedDom>,
}

#[derive(Debug, Clone)]
pub(super) struct CapturedDom {
    reply: Value,
    parsed: OnceLock<Vec<DomTree>>,
}

impl From<Value> for CapturedDom {
    fn from(reply: Value) -> Self {
        Self {
            reply,
            parsed: OnceLock::new(),
        }
    }
}

impl CapturedSnapshot {
    /// The DOM reply of an individual target in the snapshot.
    pub(crate) fn dom_reply(&self, target: &str) -> Option<&Value> {
        self.dom_captures.get(target).map(|capture| &capture.reply)
    }

    /// Parsed documents of an individual target in the snapshot.
    pub(crate) fn dom(&self, target: &str) -> Option<&[DomTree]> {
        let capture = self.dom_captures.get(target)?;
        Some(
            capture
                .parsed
                .get_or_init(|| parse_capture(&capture.reply, target)),
        )
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parsed_documents_are_cached_for_their_own_target() {
        let reply = |url| {
            json!({
                "strings": [url],
                "documents": [{ "documentURL": 0 }],
            })
        };
        let capture = CapturedSnapshot {
            snapshot_id: String::new(),
            tree: AccessibilityNode::default(),
            info: PageInfo::default(),
            ref_count: 0,
            unreadable_frames: 0,
            skipped_frames: Vec::new(),
            dom_captures: HashMap::from([
                ("main".to_owned(), reply("https://example.test").into()),
                ("frame".to_owned(), reply("https://frame.test").into()),
            ]),
        };
        let main = capture.dom("main").unwrap();
        let frame = capture.dom("frame").unwrap();
        assert_eq!(main[0].target_id, "main");
        assert_eq!(frame[0].target_id, "frame");
        assert_eq!(frame[0].url, "https://frame.test");
        assert!(std::ptr::eq(main, capture.dom("main").unwrap()));
        assert!(capture.dom("missing").is_none());
    }
}
