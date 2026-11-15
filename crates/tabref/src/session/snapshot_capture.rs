use tabref_core::{AppError, RefMap, allocate_refs, new_snapshot_id};
use tabref_page::{PageInfo, snapshot_page_within_budget};

use super::Session;
use super::captured_snapshot::CapturedSnapshot;

impl Session {
    /// Reads the current tab, gives every actionable node a ref and saves the
    /// refs as a new snapshot. Slow iframes are left out rather than costing
    /// the page.
    pub(crate) fn capture(&mut self) -> Result<CapturedSnapshot, AppError> {
        let page = self.page()?;
        let deadline = self.deadline();
        let snapshot = snapshot_page_within_budget(self.connection(), &page, deadline)?;
        let info = PageInfo::read(self.connection(), &page.target_id, deadline)?;
        let snapshot_id = new_snapshot_id();
        let mut map = RefMap::new(&page.target_id, &info.url);
        let mut tree = snapshot.tree;
        let ref_count = allocate_refs(&mut tree, &snapshot_id, &mut map);
        self.store().save(&snapshot_id, &map)?;
        Ok(CapturedSnapshot {
            snapshot_id,
            tree,
            info,
            ref_count,
            unreadable_frames: snapshot.unreadable_frames.len(),
            skipped_frames: snapshot.skipped_frames,
            dom_captures: snapshot.dom_captures,
        })
    }
}
