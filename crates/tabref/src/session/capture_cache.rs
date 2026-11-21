//! The daemon's last capture of each tab, kept so a `find`, `snap` or
//! `read` right after an `open` or `snap` of an unchanged page does not read
//! the whole page again. Actions may reuse one to resolve their target, but
//! never keep one across commands. A kept capture is used
//! only while the page's own watch says nothing changed since (see
//! [`tabref_page::check_capture_watch`]), it is the tab's latest snapshot,
//! and it is younger than [`MAX_AGE`]. In-process runs keep nothing, and
//! every in-process command that is not a read writes a new page-actions
//! stamp to the ref store, which a kept capture must still match, so an
//! in-process hover between two daemon reads is never missed. The page
//! script that checks the watch gets [`WATCH_WAIT`], allowing a large page's
//! visibility scan to finish while bounding a busy page's wait; small pages
//! return as soon as checked. The one that arms
//! it [`ARM_WAIT`], which leaves room for its wait for an idle main thread:
//! a page whose main thread is held (an open dialog, a long task) is not
//! kept or reused, and the command goes on as without the cache. At most
//! [`MAX_KEPT`] tabs keep one.
#![cfg_attr(not(unix), allow(dead_code))]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tabref_cdp::PageSession;
use tabref_core::{RefStore, new_snapshot_id};
use tabref_page::{
    CaptureWatch, IDLE_WAIT_MS, arm_capture_watch, check_capture_watch, has_closed_shadow_root,
};

use super::Session;
use super::captured_snapshot::CapturedSnapshot;
use super::ref_target::latest_snapshot_of;

const MAX_AGE: Duration = Duration::from_secs(5);
const WATCH_WAIT: Duration = Duration::from_millis(30);
const ARM_WAIT: Duration = Duration::from_millis(IDLE_WAIT_MS + 11);
const MAX_KEPT: usize = 4;
const PAGE_ACTIONS: &str = "page-actions";

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
    actions: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct CaptureAccess {
    cache: Arc<CaptureCache>,
    keep: bool,
}

impl CaptureAccess {
    fn writable(&self) -> Option<&Arc<CaptureCache>> {
        self.keep.then_some(&self.cache)
    }
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
            map.retain(|_, old| old.taken.elapsed() < MAX_AGE);
            map.insert(key, kept);
            while map.len() > MAX_KEPT {
                let oldest = map
                    .iter()
                    .min_by_key(|(_, old)| old.taken)
                    .map(|(key, _)| key.clone());
                match oldest {
                    Some(oldest) => map.remove(&oldest),
                    None => break,
                };
            }
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
        self.reuse_captures(cache);
        if let Some(access) = &mut self.captures {
            access.keep = true;
        }
    }

    /// Lets this command reuse captures without arming watches or keeping
    /// any captures it reads itself.
    pub(crate) fn reuse_captures(&mut self, cache: Arc<CaptureCache>) {
        self.captures = Some(CaptureAccess { cache, keep: false });
    }

    /// Whether this command keeps its captures for the next one to reuse.
    pub(crate) fn keeps_captures(&self) -> bool {
        self.captures
            .as_ref()
            .and_then(CaptureAccess::writable)
            .is_some()
    }

    /// A capture of the current tab: the kept one when the page provably
    /// has not changed since it was taken, else a new one.
    pub(crate) fn current_capture(
        &mut self,
    ) -> Result<Arc<CapturedSnapshot>, tabref_core::AppError> {
        match self.reusable_capture() {
            Some(captured) => Ok(captured),
            None => self.shared_capture(),
        }
    }

    /// The kept capture of the current tab, when it may stand for the page
    /// as it is now. A capture found stale is forgotten. If the check
    /// times out, this command reads fresh and the next one can check again.
    pub(crate) fn reusable_capture(&mut self) -> Option<Arc<CapturedSnapshot>> {
        let cache = Arc::clone(&self.captures.as_ref()?.cache);
        let page = self.page().ok()?;
        let key = self.capture_key(&page);
        let kept = cache.take(&key)?;
        let latest = latest_snapshot_of(self.store(), &page.target_id);
        let fresh = kept.taken.elapsed() < MAX_AGE
            && latest.as_deref() == Some(kept.captured.snapshot_id.as_str())
            && page_actions(self.store()) == kept.actions;
        if fresh {
            let deadline = self.deadline().capped(WATCH_WAIT);
            match check_capture_watch(self.connection(), &page, &kept.watch, deadline).as_str() {
                "same" => return Some(kept.captured),
                "timed_out" => return None,
                _ => {}
            }
        }
        cache.forget(&key);
        None
    }

    /// Marks that a command which may change the page ran outside the
    /// daemon, so no capture the daemon kept before it is used again.
    pub(crate) fn note_page_action(&self) {
        let _ = self.store().write_state(PAGE_ACTIONS, &new_snapshot_id());
    }

    /// Arms the page's watch before a capture, when this command keeps
    /// captures.
    pub(super) fn arm_watch(&mut self, page: &PageSession) -> Option<CaptureWatch> {
        self.captures.as_ref()?.writable()?;
        let deadline = self.deadline().capped(ARM_WAIT);
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
        captured: &Arc<CapturedSnapshot>,
        watch: Option<CaptureWatch>,
    ) {
        let Some(cache) = self
            .captures
            .as_ref()
            .and_then(CaptureAccess::writable)
            .cloned()
        else {
            return;
        };
        let key = self.capture_key(page);
        let whole = captured
            .dom_reply(&page.target_id)
            .is_some_and(|capture| !has_closed_shadow_root(capture));
        match watch {
            Some(watch) if whole => cache.put(
                key,
                Kept {
                    captured: Arc::clone(captured),
                    watch,
                    taken: Instant::now(),
                    actions: page_actions(self.store()),
                },
            ),
            _ => cache.forget(&key),
        }
    }

    fn capture_key(&self, page: &PageSession) -> (String, String) {
        (
            self.endpoint().websocket_url.as_url().to_owned(),
            page.target_id.clone(),
        )
    }
}

fn page_actions(store: &RefStore) -> Option<String> {
    store.read_state(PAGE_ACTIONS)
}

#[cfg(test)]
mod tests {
    use super::{Arc, CaptureAccess, CaptureCache};

    #[test]
    fn reuse_only_access_cannot_arm_or_keep() {
        let cache = Arc::new(CaptureCache::default());
        let access = CaptureAccess {
            cache: Arc::clone(&cache),
            keep: false,
        };
        assert!(Arc::ptr_eq(&access.cache, &cache));
        assert!(access.writable().is_none());
    }

    #[test]
    fn keeping_access_can_arm_and_keep_in_the_same_cache() {
        let cache = Arc::new(CaptureCache::default());
        let access = CaptureAccess {
            cache: Arc::clone(&cache),
            keep: true,
        };
        assert!(
            access
                .writable()
                .is_some_and(|writable| Arc::ptr_eq(writable, &cache))
        );
    }
}
