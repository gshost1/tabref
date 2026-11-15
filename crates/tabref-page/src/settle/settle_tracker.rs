use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::settle_wait::SettleWait;
use crate::CdpEvent;

const TRACKED_REQUESTS: &[&str] = &["Document", "XHR", "Fetch"];

/// The page events seen since an action, folded into what is still
/// outstanding: a main-frame navigation that has not committed or reached
/// `DOMContentLoaded`, and document, XHR and fetch requests still in flight.
#[derive(Debug)]
pub(crate) struct SettleTracker {
    main_frame: String,
    navigating: bool,
    committed: bool,
    content_loaded: bool,
    same_document: bool,
    pending: HashSet<String>,
    last_activity: Option<Instant>,
}

impl SettleTracker {
    pub(crate) fn new(main_frame: &str) -> Self {
        Self {
            main_frame: main_frame.to_owned(),
            navigating: false,
            committed: false,
            content_loaded: false,
            same_document: false,
            pending: HashSet::new(),
            last_activity: None,
        }
    }

    /// Notes a navigation that the page's own reply hinted at, such as an
    /// evaluation whose context was destroyed under it.
    pub(crate) fn expect_navigation(&mut self) {
        self.navigating = true;
    }

    pub(crate) fn absorb(&mut self, event: &CdpEvent, now: Instant) {
        let params = &event.params;
        let frame = params.get("frameId").and_then(Value::as_str);
        let main = frame.is_none_or(|frame| frame == self.main_frame);
        match event.method.as_str() {
            "Page.frameRequestedNavigation" if main => {
                let disposition = params.get("disposition").and_then(Value::as_str);
                if disposition.is_none_or(|kind| kind == "currentTab") {
                    self.navigating = true;
                }
            }
            "Page.frameScheduledNavigation" | "Page.frameStartedLoading" if main => {
                self.navigating = true;
            }
            "Page.frameStartedNavigating" if main => {
                let kind = params
                    .get("navigationType")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if kind.to_ascii_lowercase().contains("samedocument") {
                    self.same_document = true;
                } else {
                    self.navigating = true;
                }
            }
            "Page.navigatedWithinDocument" if main => self.same_document = true,
            "Page.frameNavigated" => {
                let id = params.pointer("/frame/id").and_then(Value::as_str);
                if id == Some(self.main_frame.as_str()) {
                    self.navigating = true;
                    self.committed = true;
                }
            }
            "Page.domContentEventFired" if self.committed => self.content_loaded = true,
            "Page.frameStoppedLoading" if main => {
                if self.committed {
                    self.content_loaded = true;
                } else {
                    self.navigating = false;
                }
            }
            "Network.requestWillBeSent" => {
                let kind = params.get("type").and_then(Value::as_str);
                if kind.is_some_and(|kind| TRACKED_REQUESTS.contains(&kind)) {
                    self.pending.insert(request_id(params));
                    self.last_activity = Some(now);
                }
            }
            "Network.loadingFinished" | "Network.loadingFailed" => {
                if self.pending.remove(&request_id(params)) {
                    self.last_activity = Some(now);
                }
            }
            _ => {}
        }
    }

    /// What is still outstanding at `now`: a navigation, requests in
    /// flight, or the quiet window after the last request.
    pub(crate) fn next_wait(&self, now: Instant, quiet: Duration) -> SettleWait {
        if self.navigating && !(self.committed && self.content_loaded) {
            return SettleWait::Navigation;
        }
        if !self.pending.is_empty() {
            return SettleWait::Network;
        }
        match self.last_activity {
            Some(at) if now < at + quiet => SettleWait::QuietUntil(at + quiet),
            _ => SettleWait::Done,
        }
    }

    pub(crate) fn navigating(&self) -> bool {
        self.navigating
    }

    pub(crate) fn committed(&self) -> bool {
        self.committed
    }

    pub(crate) fn same_document(&self) -> bool {
        self.same_document
    }

    pub(crate) fn saw_network(&self) -> bool {
        self.last_activity.is_some()
    }
}

fn request_id(params: &Value) -> String {
    params
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
#[path = "settle_tracker_tests.rs"]
mod tests;
