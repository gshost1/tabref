//! Turns what the frame reader collected into frames ready to graft: each
//! read frame built into nodes, beside its parent frame and owner element.
//! A child frame that was read but holds no text and no control, and holds
//! no frame of its own, is left out as empty; frames inside a frame that was
//! left out go with it.

use std::collections::HashSet;

use tabref_core::AccessibilityNode;

use super::dom_facts::DomFacts;
use super::frame_build::build_frame;
use super::frame_context::FrameContext;
use super::frame_graft::FrameNodes;
use super::frame_reader::FrameReader;
use super::frame_skip_reason::FrameSkipReason;
use super::web_node::WebNode;

const WEB_AREA: &str = "webarea";

impl FrameReader {
    /// The frames that are kept, the page's main frame first and every
    /// parent before its children.
    pub(crate) fn assemble(&mut self) -> Vec<FrameNodes> {
        let mut built = Vec::new();
        for target in 0..self.targets.len() {
            if target > 0 && self.is_closed_target(target) {
                continue;
            }
            self.build_target(target, &mut built);
        }
        let parents: HashSet<String> = built
            .iter()
            .filter_map(|frame: &FrameNodes| frame.parent_id.clone())
            .collect();
        let mut kept = Vec::with_capacity(built.len());
        for (index, frame) in built.into_iter().enumerate() {
            let empty =
                index > 0 && !parents.contains(&frame.frame_id) && !has_content(&frame.nodes);
            if empty {
                let url = self.url_of(&frame.frame_id);
                self.leave_out(&frame.frame_id, url, FrameSkipReason::Empty);
                continue;
            }
            kept.push(frame);
        }
        kept
    }

    fn build_target(&mut self, target: usize, built: &mut Vec<FrameNodes>) {
        let Some(frames) = self.targets[target].frames.clone() else {
            if target > 0 {
                let (id, url) = self.identity(target);
                self.leave_out(&id, url, FrameSkipReason::TimedOut);
            }
            return;
        };
        for (index, entry) in frames.iter().enumerate() {
            if self.closed.contains(&entry.frame_id) {
                continue;
            }
            if entry
                .parent_id
                .as_ref()
                .is_some_and(|parent| index > 0 && self.closed.contains(parent))
            {
                self.closed.insert(entry.frame_id.clone());
                continue;
            }
            let key = if index == 0 {
                ""
            } else {
                entry.frame_id.as_str()
            };
            let Some(nodes) = self.targets[target].ax.remove(key) else {
                if index == 0 {
                    let (id, url) = self.identity(target);
                    self.leave_out(&id, url, FrameSkipReason::TimedOut);
                    return;
                }
                let url = entry.url.clone();
                self.leave_out(&entry.frame_id, url, FrameSkipReason::TimedOut);
                continue;
            };
            let state = &self.targets[target];
            let (parent_id, owner) = match (index, &state.parent) {
                (0, Some((parent, frame))) => (
                    entry.parent_id.clone().or_else(|| frame.clone()),
                    self.targets[*parent]
                        .owners
                        .get(&state.session.target_id)
                        .or_else(|| self.targets[*parent].owners.get(&entry.frame_id))
                        .copied()
                        .flatten(),
                ),
                (0, None) => (entry.parent_id.clone(), None),
                _ => (
                    entry.parent_id.clone(),
                    state.owners.get(&entry.frame_id).copied().flatten(),
                ),
            };
            let default_dom = DomFacts::default();
            let dom = state.dom.as_ref().map_or(&default_dom, |dom| &dom.facts);
            let context = FrameContext {
                target_id: &state.session.target_id,
                frame_id: &entry.frame_id,
                loader_id: &entry.loader_id,
                dom,
            };
            built.push(FrameNodes {
                frame_id: entry.frame_id.clone(),
                parent_id,
                owner_backend_node_id: owner,
                nodes: build_frame(nodes, &context),
            });
        }
    }

    fn identity(&self, target: usize) -> (String, String) {
        let state = &self.targets[target];
        (state.session.target_id.clone(), state.url.clone())
    }

    fn url_of(&self, frame_id: &str) -> String {
        self.targets
            .iter()
            .find(|state| {
                state.session.target_id == frame_id
                    || state
                        .frames
                        .as_ref()
                        .is_some_and(|frames| frames.iter().any(|frame| frame.frame_id == frame_id))
            })
            .map(|state| state.frame_url(frame_id))
            .unwrap_or_default()
    }
}

/// Whether a frame's nodes hold any text or control, its own document node
/// aside.
fn has_content(nodes: &[WebNode]) -> bool {
    nodes.iter().any(|node| {
        let own = node.node.role != WEB_AREA && shows_something(&node.node);
        own || node.children.iter().any(holds_content)
    })
}

fn holds_content(node: &WebNode) -> bool {
    shows_something(&node.node) || node.children.iter().any(holds_content)
}

fn shows_something(node: &AccessibilityNode) -> bool {
    node.display_name().is_some()
        || node
            .identity
            .value
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || !node.presentation.available_actions.is_empty()
}
