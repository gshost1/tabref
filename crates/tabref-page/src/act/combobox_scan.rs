use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;
use tabref_core::{AccessibilityNode, AppError, ErrorCode};

use crate::locate::name_match::rank;
use crate::{DomTree, ElementToken, cut_chars};

const STABLE_WAIT: Duration = Duration::from_millis(350);

#[derive(Clone, Copy, Deserialize)]
pub(super) struct ScrollPosition {
    pub top: f64,
    pub end: bool,
    pub height: f64,
    scale: f64,
}

pub(super) enum Observation {
    Reset,
    Advance,
    Wait,
    Complete,
}

#[derive(Default)]
pub(super) struct Scan {
    pub popup: Option<i64>,
    pub options: AccessibilityNode,
    entries: BTreeMap<OptionPosition, SeenOption>,
    original_top: Option<f64>,
    last_top: Option<f64>,
    unchanged_since: Option<Instant>,
    gap: bool,
    complete: bool,
    seen: usize,
}

impl Scan {
    pub fn observe(
        &mut self,
        popup: i64,
        options: AccessibilityNode,
        dom: &DomTree,
        position: Option<ScrollPosition>,
    ) -> Observation {
        if self.popup != Some(popup) {
            *self = Self {
                popup: Some(popup),
                ..Self::default()
            };
        }
        let unchanged = self.options == options;
        self.options = options;
        let Some(position) = position else {
            if !unchanged {
                self.unchanged_since = Some(Instant::now());
            }
            let stable = self
                .unchanged_since
                .get_or_insert_with(Instant::now)
                .elapsed()
                >= STABLE_WAIT;
            self.entries.clear();
            self.record(dom, None);
            if !self.options.children.is_empty() && stable {
                self.complete = true;
                return Observation::Complete;
            }
            return Observation::Wait;
        };

        if self.original_top.is_none() {
            self.original_top = Some(position.top);
            if position.top > 0.0 {
                return Observation::Reset;
            }
        }
        if self.last_top.is_none() && position.top > 0.0 {
            return Observation::Reset;
        }
        let same_window = self.last_top == Some(position.top);
        self.gap |= self
            .last_top
            .is_some_and(|last| position.top > last + position.height);
        self.last_top = Some(position.top);
        self.record(dom, Some(position));
        if position.end {
            if !same_window || !unchanged {
                self.unchanged_since = Some(Instant::now());
            }
            if self
                .unchanged_since
                .get_or_insert_with(Instant::now)
                .elapsed()
                >= STABLE_WAIT
            {
                self.complete = true;
                Observation::Complete
            } else {
                Observation::Wait
            }
        } else {
            self.unchanged_since = None;
            Observation::Advance
        }
    }

    fn record(&mut self, dom: &DomTree, position: Option<ScrollPosition>) {
        for option in &self.options.children {
            let key = OptionPosition::of(option, dom, position);
            if self
                .entries
                .get(&key)
                .is_none_or(|seen| seen.option.display_name() != option.display_name())
            {
                self.entries.insert(
                    key.clone(),
                    SeenOption {
                        option: option.clone(),
                        top: position.map(|pos| pos.top),
                        key,
                    },
                );
            }
        }
        self.seen = self.seen.max(self.entries.len());
    }

    pub fn has_options(&self) -> bool {
        self.seen > 0
    }

    pub fn original_scroll(&self) -> Option<(i64, f64)> {
        self.popup.zip(self.original_top)
    }

    pub fn incomplete(&self) -> bool {
        self.gap || (!self.complete && (self.original_top.is_some() || self.has_options()))
    }

    pub fn incomplete_error(&self) -> AppError {
        let seen = self.seen;
        AppError::new(
            ErrorCode::ElementNotFound,
            format!("list not fully searched (saw {seen} options)"),
        )
        .with_details(json!({ "kind": "combobox_list_incomplete", "options_seen": seen }))
    }

    pub fn describe_missing(&self, mut error: AppError) -> AppError {
        if error.code != ErrorCode::ElementNotFound {
            return error;
        }
        let names: BTreeSet<_> = self
            .entries
            .values()
            .filter_map(|entry| entry.option.display_name().map(|name| cut_chars(name, 60)))
            .collect();
        let shown: Vec<_> = names.iter().take(30).collect();
        if shown.is_empty() {
            error.message.push_str("; no popup options appeared");
        } else {
            let listed = shown
                .iter()
                .map(|name| name.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            error.message.push_str(&format!("; options: {listed}"));
            if names.len() > shown.len() {
                error
                    .message
                    .push_str(&format!(" … and {} more", names.len() - shown.len()));
            }
        }
        if let Some(details) = &mut error.details {
            details["options"] = json!(shown);
            details["options_total"] = json!(self.entries.len());
        }
        error
    }

    pub fn matches(&self, query: &str) -> Vec<&SeenOption> {
        self.entries
            .values()
            .filter(|entry| rank(&entry.option, query).is_some())
            .collect()
    }
}

pub(super) struct SeenOption {
    pub option: AccessibilityNode,
    pub top: Option<f64>,
    key: OptionPosition,
}

impl SeenOption {
    pub fn find(
        &self,
        mut options: AccessibilityNode,
        dom: &DomTree,
        position: Option<ScrollPosition>,
    ) -> Option<AccessibilityNode> {
        options.children.retain(|option| {
            option.display_name() == self.option.display_name()
                && OptionPosition::of(option, dom, position) == self.key
        });
        (options.children.len() == 1).then_some(options)
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum OptionPosition {
    Index(u64),
    Bounds(i64, i64),
    Backend(i64),
}

impl OptionPosition {
    fn of(option: &AccessibilityNode, dom: &DomTree, position: Option<ScrollPosition>) -> Self {
        let backend = option
            .identity
            .retained_object
            .as_deref()
            .and_then(|token| ElementToken::parse(token).ok())
            .map_or(0, |token| token.backend_node_id);
        if let Some(position) = position
            && let Some(node) = dom.find_backend(backend).and_then(|index| dom.node(index))
        {
            if let Some(index) = node
                .attr("aria-posinset")
                .and_then(|value| value.parse().ok())
            {
                return Self::Index(index);
            }
            if let Some(layout) = &node.layout {
                let [x, y, ..] = layout.bounds;
                return Self::Bounds(
                    x.round() as i64,
                    (y + position.top * position.scale).round() as i64,
                );
            }
        }
        Self::Backend(backend)
    }
}
