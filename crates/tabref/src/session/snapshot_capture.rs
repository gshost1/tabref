use tabref_core::{AppError, new_snapshot_id};
use tabref_page::{PageInfo, snapshot_page_within_budget};

use super::Session;
use super::captured_snapshot::CapturedSnapshot;

impl Session {
    /// Reads the current tab, gives every actionable node a ref (keeping
    /// the number an element had in the tab's earlier snapshots) and saves
    /// the refs as a new snapshot. Slow iframes are left out rather than costing
    /// the page.
    pub(crate) fn capture(&mut self) -> Result<CapturedSnapshot, AppError> {
        let page = self.page()?;
        let deadline = self.deadline();
        let snapshot = snapshot_page_within_budget(self.connection(), &page, deadline)?;
        let info = PageInfo::read(self.connection(), &page.target_id, deadline)?;
        let snapshot_id = new_snapshot_id();
        let mut map = self.continued_refs(&page.target_id, &info.url);
        let mut tree = snapshot.tree;
        let ref_count = self.number_refs(&mut tree, &snapshot_id, &mut map)?;
        self.store().save(&snapshot_id, &map)?;
        self.remember_latest(&page.target_id, &snapshot_id)?;
        Ok(CapturedSnapshot {
            snapshot_id,
            tree,
            info,
            ref_count,
            unreadable_frames: snapshot.unreadable_frames.len(),
        })
    }
}
