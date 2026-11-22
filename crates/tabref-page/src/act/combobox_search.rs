use std::collections::BTreeSet;
use std::time::Duration;

use serde_json::json;
use tabref_cdp::{is_timeout, transport_failed};
use tabref_core::{AccessibilityNode, AppError, Deadline, ErrorCode, NodeIdentity};

use super::combobox::frame_dom;
use crate::act::element_call::ElementCall;
use crate::act::remote_element::call_on_element;
use crate::locate::name_match::{EXACT_NAME, normalize, rank};
use crate::tree::ax_nodes::AxNodes;
use crate::tree::web_roles::map_cdp_role;
use crate::{
    CdpConnection, DomNode, DomTree, ElementToken, LocateIntent, Located, Locator, PageSession,
    cut_chars, locate,
};

const POPUP_WAIT: Duration = Duration::from_millis(900);
const POLL: Duration = Duration::from_millis(50);
const POPUP_ROLES: &[&str] = &["listbox", "menu", "grid", "tree"];

const SCROLL: &str = r"function(top) {
  const scrollable = node => node.scrollHeight > node.clientHeight &&
    /^(auto|scroll|overlay)$/.test(getComputedStyle(node).overflowY);
  const pane = scrollable(this) ? this : Array.from(this.children).find(scrollable);
  if (!pane) return null;
  const old = pane.scrollTop;
  const step = Math.max(1, pane.clientHeight * 0.8);
  pane.scrollTo({ top: top ?? old + step, behavior: 'instant' });
  return { old, now: pane.scrollTop };
}";

pub(super) struct PopupSearch<'a> {
    pub session: &'a PageSession,
    pub combo: &'a ElementToken,
    pub baseline: BTreeSet<i64>,
    pub locator: Locator,
}

impl PopupSearch<'_> {
    pub fn find(
        &self,
        connection: &mut CdpConnection,
        seen: &mut BTreeSet<String>,
        deadline: Deadline,
    ) -> Result<(Located, i64), AppError> {
        let wait = deadline.capped(POPUP_WAIT.min(deadline.remaining() / 2));
        let query = normalize(self.locator.name.as_deref().unwrap_or_default());
        let mut options = AccessibilityNode::default();
        let mut popup = None;
        let mut fallback = None;
        let mut started_at_top = None;
        loop {
            let current = self.poll(connection, wait);
            let (current_popup, current_options) = match current {
                Ok(snapshot) => snapshot,
                Err(error) if is_timeout(&error) && wait.is_expired() => break,
                Err(error) => return Err(error),
            };
            seen.extend(
                current_options
                    .children
                    .iter()
                    .filter_map(|option| option.display_name().map(|name| cut_chars(name, 60))),
            );
            let stable = popup == current_popup && options == current_options;
            popup = current_popup;
            options = current_options;
            if options
                .children
                .iter()
                .any(|option| rank(option, &query) == Some(EXACT_NAME))
            {
                return self.pick(&options, popup);
            }
            if wait.remaining() <= POLL {
                break;
            }
            if let Some(backend) = popup
                && stable
                && !options.children.is_empty()
            {
                let to = (started_at_top != Some(backend)).then_some(0.0);
                let scroll = match self.scroll(connection, backend, to, wait) {
                    Ok(scroll) => scroll,
                    Err(error) if is_timeout(&error) && wait.is_expired() => break,
                    Err(error) => return Err(error),
                };
                started_at_top = Some(backend);
                if let Some(top) = scroll
                    && options
                        .children
                        .iter()
                        .any(|option| rank(option, &query).is_some())
                {
                    fallback.get_or_insert((backend, top));
                }
            }
            std::thread::sleep(POLL);
        }
        if options
            .children
            .iter()
            .all(|option| rank(option, &query).is_none())
            && let Some((backend, top)) = fallback
            && popup == Some(backend)
        {
            self.scroll(connection, backend, Some(top), deadline)?;
            let restore = deadline.capped(POLL * 3);
            while restore.remaining() > POLL {
                std::thread::sleep(POLL);
                (popup, options) = self.poll(connection, restore)?;
                if options
                    .children
                    .iter()
                    .any(|option| rank(option, &query).is_some())
                {
                    break;
                }
            }
        }
        self.pick(&options, popup)
    }

    fn pick(
        &self,
        options: &AccessibilityNode,
        popup: Option<i64>,
    ) -> Result<(Located, i64), AppError> {
        let option = locate(options, &self.locator, LocateIntent::Click).map_err(|error| {
            if error.code == ErrorCode::AmbiguousTarget {
                self.ambiguous(options, error)
            } else {
                error
            }
        })?;
        let backend = popup.ok_or_else(|| AppError::internal("an option has no popup"))?;
        Ok((option, backend))
    }

    fn ambiguous(&self, options: &AccessibilityNode, mut error: AppError) -> AppError {
        let query = self.locator.name.as_deref().unwrap_or_default();
        let normalized = normalize(query);
        let matches = options
            .children
            .iter()
            .filter_map(|option| Some((rank(option, &normalized)?, option.display_name()?)));
        let best = matches.clone().map(|(rank, _)| rank).min();
        let names: Vec<_> = matches
            .filter(|(rank, _)| Some(*rank) == best)
            .take(30)
            .map(|(_, name)| format!("\"{}\"", crate::escape_line(name, true)))
            .collect();
        error.message = format!(
            "multiple options match {query:?}: {}; use a full option name with select. For duplicate names, open the combobox and use tabref snap to get refs for tabref click",
            names.join(" | ")
        );
        error
    }

    fn poll(
        &self,
        connection: &mut CdpConnection,
        deadline: Deadline,
    ) -> Result<(Option<i64>, AccessibilityNode), AppError> {
        let dom = frame_dom(connection, self.session, self.combo, deadline)?;
        let Some(popup) = self.popup(&dom, None) else {
            return Ok((None, AccessibilityNode::default()));
        };
        let options = self.options(connection, popup.backend_id, deadline)?;
        Ok((Some(popup.backend_id), options))
    }

    pub fn token(&self, backend: i64) -> ElementToken {
        ElementToken {
            backend_node_id: backend,
            ..self.combo.clone()
        }
    }

    fn scroll(
        &self,
        connection: &mut CdpConnection,
        backend: i64,
        to: Option<f64>,
        deadline: Deadline,
    ) -> Result<Option<f64>, AppError> {
        let reply = call_on_element(
            connection,
            self.session,
            &self.token(backend),
            ElementCall {
                function: SCROLL,
                arguments: vec![json!(to)],
            },
            deadline,
        )?;
        Ok(reply["old"]
            .as_f64()
            .filter(|_| reply["old"] != reply["now"]))
    }

    pub fn options(
        &self,
        connection: &mut CdpConnection,
        backend: i64,
        deadline: Deadline,
    ) -> Result<AccessibilityNode, AppError> {
        let reply = connection.call(
            "Accessibility.queryAXTree",
            json!({ "backendNodeId": backend }),
            Some(&self.session.session_id),
            deadline,
        )?;
        let nodes: AxNodes = serde_json::from_value(reply)
            .map_err(|error| transport_failed("Accessibility.queryAXTree", error.to_string()))?;
        let children = nodes
            .nodes
            .into_iter()
            .filter(|node| !node.ignored && map_cdp_role(&node.raw_role()) == "option")
            .filter_map(|node| {
                let token = self.token(node.backend_dom_node_id?);
                Some(AccessibilityNode {
                    role: "option".to_owned(),
                    identity: NodeIdentity {
                        retained_object: Some(token.to_string()),
                        name: node.text.name.and_then(|value| value.as_text()),
                        description: node.text.description.and_then(|value| value.as_text()),
                        ..NodeIdentity::default()
                    },
                    ..AccessibilityNode::default()
                })
            })
            .collect();
        Ok(AccessibilityNode {
            children,
            ..AccessibilityNode::default()
        })
    }

    pub fn popup<'d>(&self, dom: &'d DomTree, known: Option<i64>) -> Option<&'d DomNode> {
        let (declared, linked) = linked_popup(dom, self.combo);
        if declared {
            return linked;
        }
        if let Some(backend) = known {
            return dom
                .find_backend(backend)
                .and_then(|at| dom.node(at))
                .filter(|node| node.is_shown());
        }
        one(dom
            .nodes
            .iter()
            .filter(|node| is_popup(node) && !self.baseline.contains(&node.backend_id)))
    }

    pub fn open(&self, dom: &DomTree, known: Option<i64>) -> bool {
        let index = dom.find_backend(self.combo.backend_node_id);
        let expanded = index
            .and_then(|at| dom.node(at))
            .and_then(|node| node.attr("aria-expanded"));
        expanded == Some("true")
            || self.popup(dom, known).is_some()
            || (known.is_none()
                && !linked_popup(dom, self.combo).0
                && dom
                    .nodes
                    .iter()
                    .any(|node| is_popup(node) && !self.baseline.contains(&node.backend_id)))
    }
}

pub(super) fn initially_open<'d>(dom: &'d DomTree, combo: &ElementToken) -> Option<&'d DomNode> {
    let (declared, popup) = linked_popup(dom, combo);
    if declared {
        return popup;
    }
    let node = dom.node(dom.find_backend(combo.backend_node_id)?)?;
    if node.attr("aria-expanded") == Some("true") {
        one(dom.nodes.iter().filter(|node| is_popup(node)))
    } else {
        None
    }
}

fn linked_popup<'d>(dom: &'d DomTree, combo: &ElementToken) -> (bool, Option<&'d DomNode>) {
    let Some(node) = dom
        .find_backend(combo.backend_node_id)
        .and_then(|at| dom.node(at))
    else {
        return (false, None);
    };
    let mut declared = false;
    for id in ["aria-controls", "aria-owns"]
        .into_iter()
        .filter_map(|attr| node.attr(attr))
        .flat_map(str::split_whitespace)
    {
        if let Some(target) = dom.nodes.iter().find(|node| node.attr("id") == Some(id)) {
            declared = true;
            if target.is_shown() {
                return (true, Some(target));
            }
        }
    }
    (declared, None)
}

fn one<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}

pub(super) fn is_popup(node: &DomNode) -> bool {
    node.is_shown()
        && node
            .attr("role")
            .and_then(|roles| roles.split_whitespace().next())
            .is_some_and(|role| POPUP_ROLES.contains(&role))
}
