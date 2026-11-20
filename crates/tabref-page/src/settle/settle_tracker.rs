use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::response_bytes::{ReplyBody, ResponseBytes};
use super::settle_wait::SettleWait;
use crate::CdpEvent;

const TRACKED_REQUESTS: &[&str] = &["Document", "XHR", "Fetch"];

/// How long a request whose promised bytes have all arrived still holds the
/// settle when Chrome has not reported it finished. Chrome withholds
/// `loadingFinished` for a `no-store` reply until the page reads the body,
/// and a page that reads at once gets it within a millisecond of the last
/// byte, so the grace only runs out for a body read late or never; a page
/// that reads within it is reported after it renders.
pub(crate) const BODY_GRACE: Duration = Duration::from_millis(200);

/// The page events seen since an action, folded into what is still
/// outstanding: a main-frame navigation that has not committed or reached
/// `DOMContentLoaded`, and document, XHR and fetch requests that a document
/// of this page sent to its own site and that are still in flight. A
/// navigation's document request always counts. Scroll settling tracks XHR
/// and fetch requests from any origin. When all the promised response bytes
/// arrive, the request gets a bounded grace period to finish and render.
#[derive(Debug)]
pub(crate) struct SettleTracker {
    main_frame: String,
    navigating: bool,
    committed: bool,
    content_loaded: bool,
    same_document: bool,
    pending: HashSet<String>,
    bodies: HashMap<String, ResponseBytes>,
    arrived: HashMap<String, Instant>,
    first_scroll_request: Option<f64>,
    last_activity: Option<Instant>,
    scroll_started: Option<f64>,
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
            bodies: HashMap::new(),
            arrived: HashMap::new(),
            first_scroll_request: None,
            last_activity: None,
            scroll_started: None,
        }
    }

    /// Scroll loaders can fetch from an API on a different origin.
    pub(crate) fn for_scroll(browser_started: f64) -> Self {
        Self {
            scroll_started: Some(browser_started),
            ..Self::new("")
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
                let id = request_id(params);
                self.bodies.remove(&id);
                self.arrived.remove(&id);
                let kind = params.get("type").and_then(Value::as_str);
                let tracked = if let Some(started) = self.scroll_started {
                    let sent = params["wallTime"].as_f64();
                    let Some(sent) = sent.filter(|at| *at >= started) else {
                        return;
                    };
                    if !matches!(kind, Some("XHR" | "Fetch")) {
                        return;
                    }
                    let elapsed = sent - started;
                    self.first_scroll_request = Some(
                        self.first_scroll_request
                            .map_or(elapsed, |first| first.min(elapsed)),
                    );
                    true
                } else {
                    kind.is_some_and(|kind| TRACKED_REQUESTS.contains(&kind)) && same_site(params)
                };
                if tracked {
                    self.pending.insert(id);
                    self.last_activity = Some(now);
                }
            }
            "Network.responseReceived" if is_attachment(params) => {
                if self.pending.remove(&request_id(params)) {
                    self.last_activity = Some(now);
                }
                if main && !self.committed {
                    self.navigating = false;
                }
            }
            "Network.responseReceived" => {
                let id = request_id(params);
                if self.pending.contains(&id) {
                    match ReplyBody::of(params) {
                        ReplyBody::Empty => self.finish(&id, now),
                        ReplyBody::Counted(bytes) => {
                            self.bodies.insert(id, bytes);
                        }
                        ReplyBody::Unknown => {}
                    }
                }
            }
            "Network.dataReceived" => {
                let id = request_id(params);
                if self
                    .bodies
                    .get_mut(&id)
                    .is_some_and(|bytes| bytes.add(params))
                {
                    self.bodies.remove(&id);
                    self.arrived.insert(id, now);
                    self.last_activity = Some(now);
                }
            }
            "Network.loadingFinished" | "Network.loadingFailed" => {
                self.finish(&request_id(params), now);
            }
            _ => {}
        }
    }

    fn finish(&mut self, id: &str, now: Instant) {
        self.bodies.remove(id);
        self.arrived.remove(id);
        if self.pending.remove(id) {
            self.last_activity = Some(now);
        }
    }

    /// What is still outstanding at `now`: a navigation, requests in
    /// flight, the grace of requests whose bytes have all arrived, or the
    /// quiet window after the last request.
    pub(crate) fn next_wait(&self, now: Instant, quiet: Duration) -> SettleWait {
        if self.navigating && !(self.committed && self.content_loaded) {
            return SettleWait::Navigation;
        }
        let mut grace_ends: Option<Instant> = None;
        for id in &self.pending {
            let Some(at) = self.arrived.get(id) else {
                return SettleWait::Network;
            };
            let ends = *at + BODY_GRACE;
            if now < ends {
                grace_ends = Some(grace_ends.map_or(ends, |other| other.max(ends)));
            }
        }
        if let Some(ends) = grace_ends {
            return SettleWait::QuietUntil(ends);
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

    pub(crate) fn saw_scroll_request_within(&self, grace: Duration) -> bool {
        self.first_scroll_request
            .is_some_and(|elapsed| elapsed <= grace.as_secs_f64())
    }
}

/// Whether a document's response is an attachment, which the browser
/// saves as a download instead of showing: its body can take as long as
/// the file does, and says nothing about the page.
fn is_attachment(params: &Value) -> bool {
    params.get("type").and_then(Value::as_str) == Some("Document")
        && params
            .pointer("/response/headers")
            .and_then(Value::as_object)
            .is_some_and(|headers| {
                headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("content-disposition")
                        && value.as_str().is_some_and(|value| {
                            value
                                .trim_start()
                                .to_ascii_lowercase()
                                .starts_with("attachment")
                        })
                })
            })
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

fn same_site(params: &Value) -> bool {
    if params.get("type").and_then(Value::as_str) == Some("Document") {
        return true;
    }
    let request = params
        .get("request")
        .and_then(|request| request.get("url"))
        .and_then(Value::as_str);
    let Some(document) = params.get("documentURL").and_then(Value::as_str) else {
        return true;
    };
    matches!((request.and_then(site), site(document)), (Some(request), Some(document)) if request == document)
}

fn site(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if host.starts_with('[') {
        host.split_once(']').map_or(host, |(inside, _)| inside)
    } else {
        host.split(':').next().unwrap_or(host)
    };
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    if labels.len() <= 2
        || labels
            .iter()
            .all(|label| label.chars().all(|c| c.is_ascii_digit()))
    {
        return Some(host.to_ascii_lowercase());
    }
    Some(labels[labels.len() - 2..].join(".").to_ascii_lowercase())
}
