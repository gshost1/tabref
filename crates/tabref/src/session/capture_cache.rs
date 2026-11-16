//! The daemon's last capture of each tab, kept so a `find`, `snap` or
//! `read` right after an `open` or `snap` of an unchanged page does not read
//! the whole page again. Only reading commands keep or use one; every other
//! command clears them all, before and after it runs. A kept capture is used
//! only while the page's own watch says nothing changed since (see
//! [`tabref_page::check_capture_watch`]), it is the tab's latest snapshot,
//! and it is younger than [`MAX_AGE`]. In-process runs keep nothing.
#![cfg_attr(not(unix), allow(dead_code))]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tabref_cdp::PageSession;
use tabref_core::new_snapshot_id;
use tabref_page::{CaptureWatch, arm_capture_watch, check_capture_watch, has_closed_shadow_root};

use super::Session;
use super::captured_snapshot::CapturedSnapshot;
use super::ref_target::latest_snapshot_of;

const MAX_AGE: Duration = Duration::from_secs(60);

/// The kept captures, by browser and tab, shared by the daemon's sessions.
#[derive(Debug, Default)]
pub(crate) struct CaptureCache {
    kept: Mutex<HashMap<(String, String), Kept>>,
}

#[derive(Debug, Clone)]
struct Kept {
    captured: Arc<CapturedSnapshot>,
    watch: CaptureWatch,
    taken: Instant,
}

impl CaptureCache {
    /// Forgets every kept capture.
    pub(crate) fn clear(&self) {
        if let Ok(mut kept) = self.kept.lock() {
            kept.clear();
        }
    }

    fn take(&self, key: &(String, String)) -> Option<Kept> {
        self.kept.lock().ok()?.get(key).cloned()
    }

    fn put(&self, key: (String, String), kept: Kept) {
        if let Ok(mut map) = self.kept.lock() {
            map.insert(key, kept);
        }
    }

    fn forget(&self, key: &(String, String)) {
        if let Ok(mut map) = self.kept.lock() {
            map.remove(key);
        }
    }
}

impl Session {
    /// Lets this command keep and reuse captures in `cache`.
    pub(crate) fn keep_captures(&mut self, cache: Arc<CaptureCache>) {
        self.captures = Some(cache);
    }

    /// A capture of the current tab: the kept one when the page provably
    /// has not changed since it was taken, else a new one.
    pub(crate) fn current_capture(&mut self) -> Result<CapturedSnapshot, tabref_core::AppError> {
        match self.reusable_capture() {
            Some(captured) => Ok(captured),
            None => self.capture(),
        }
    }

    /// The kept capture of the current tab, when it may stand for the page
    /// as it is now. A capture found stale is forgotten.
    pub(crate) fn reusable_capture(&mut self) -> Option<CapturedSnapshot> {
        let cache = self.captures.clone()?;
        let page = self.page().ok()?;
        let key = self.capture_key(&page);
        let kept = cache.take(&key)?;
        let latest = latest_snapshot_of(self.store(), &page.target_id);
        let fresh = kept.taken.elapsed() < MAX_AGE
            && latest.as_deref() == Some(kept.captured.snapshot_id.as_str());
        let deadline = self.deadline();
        if fresh && check_capture_watch(self.connection(), &page, &kept.watch, deadline) == "same" {
            return Some(CapturedSnapshot::clone(&kept.captured));
        }
        cache.forget(&key);
        None
    }

    /// Arms the page's watch before a capture, when this command keeps
    /// captures.
    pub(super) fn arm_watch(&mut self, page: &PageSession) -> Option<CaptureWatch> {
        self.captures.as_ref()?;
        let deadline = self.deadline();
        let token = new_snapshot_id();
        arm_capture_watch(self.connection(), page, &token, deadline)
            .ok()
            .filter(|watch| !watch.blind)
    }

    /// Keeps `captured` for later commands, unless its watch could not see
    /// the whole page.
    pub(super) fn keep(
        &mut self,
        page: &PageSession,
        captured: &CapturedSnapshot,
        watch: Option<CaptureWatch>,
    ) {
        let Some(cache) = self.captures.clone() else {
            return;
        };
        let key = self.capture_key(page);
        let whole = captured
            .dom_captures
            .get(&page.target_id)
            .is_some_and(|capture| !has_closed_shadow_root(capture));
        match watch {
            Some(watch) if whole => cache.put(
                key,
                Kept {
                    captured: Arc::new(captured.clone()),
                    watch,
                    taken: Instant::now(),
                },
            ),
            _ => cache.forget(&key),
        }
    }

    fn capture_key(&self, page: &PageSession) -> (String, String) {
        (
            self.endpoint.websocket_url.as_url().to_owned(),
            page.target_id.clone(),
        )
    }
}
