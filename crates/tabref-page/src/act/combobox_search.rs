use std::collections::BTreeSet;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
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
    locate,
};

#[path = "combobox_scan.rs"]
mod scan;
use scan::{Observation, Scan, ScrollPosition};

const POPUP_WAIT: Duration = Duration::from_millis(900);
const POLL: Duration = Duration::from_millis(40);
const POPUP_ROLES: &[&str] = &["listbox", "menu", "grid", "tree"];

const SCROLL: &str = r"function(to) {
  const scrollable = node => node.clientHeight > 0 && node.scrollHeight > node.clientHeight &&
    /^(auto|scroll|overlay)$/.test(getComputedStyle(node).overflowY);
  const pane = scrollable(this) ? this : Array.from(this.querySelectorAll('*')).find(scrollable);
  if (!pane) return null;
  if (to !== null) {
    const step = Math.max(1, pane.clientHeight * 0.8);
    pane.scrollTo({ top: to === 'next' ? pane.scrollTop + step : to, behavior: 'instant' });
  }
  return { top: pane.scrollTop, end: pane.scrollTop + pane.clientHeight >= pane.scrollHeight - 1,
    height: pane.clientHeight, scale: this.ownerDocument.defaultView.devicePixelRatio };
}";

pub(super) struct PopupSearch<'a> {
    pub session: &'a PageSession,
    pub combo: &'a ElementToken,
    pub baseline: BTreeSet<i64>,
    pub locator: Locator,
}

pub(super) enum SearchMode {
    All,
    ReserveFilter,
    ExactAfterIncomplete(AppError),
}

impl PopupSearch<'_> {
    pub fn find(
        &self,
        connection: &mut CdpConnection,
        deadline: Deadline,
        mode: SearchMode,
    ) -> Result<(Located, i64), AppError> {
        let remaining = deadline.remaining();
        let reserve = match mode {
            SearchMode::ReserveFilter => remaining / 2,
            _ => (remaining / 3).min(Duration::from_millis(350)),
        };
        let wait = deadline.capped(remaining.saturating_sub(reserve));
        let query = normalize(self.locator.name.as_deref().unwrap_or_default());
        let mut scan = Scan::default();
        match self.browse(connection, &query, &mut scan, wait) {
            Ok(Some(pick)) => return Ok(pick),
            Err(error) if !is_timeout(&error) || !wait.is_expired() => return Err(error),
            _ => {}
        }

        let result = if let SearchMode::ExactAfterIncomplete(original) = mode {
            Err(original)
        } else if scan.incomplete() {
            Err(scan.incomplete_error())
        } else {
            self.finish_scan(connection, &scan, &query, deadline)
                .map_err(|error| scan.describe_missing(error))
        };
        if result.is_err()
            && let Some((backend, top)) = scan.original_scroll()
        {
            let _ = self.scroll(connection, backend, json!(top), deadline);
        }
        result
    }

    fn browse(
        &self,
        connection: &mut CdpConnection,
        query: &str,
        scan: &mut Scan,
        wait: Deadline,
    ) -> Result<Option<(Located, i64)>, AppError> {
        let discovery = wait.capped(POPUP_WAIT);
        while wait.remaining() > POLL {
            let dom = frame_dom(connection, self.session, self.combo, wait)?;
            let Some(popup) = self.popup(&dom, None).map(|node| node.backend_id) else {
                if !scan.has_options() && discovery.is_expired() {
                    break;
                }
                std::thread::sleep(POLL);
                continue;
            };
            let options = self.options(connection, popup, wait)?;
            if options
                .children
                .iter()
                .any(|option| rank(option, query) == Some(EXACT_NAME))
            {
                return self.pick(&options, popup).map(Some);
            }
            let position = self.scroll(connection, popup, Value::Null, wait)?;
            match scan.observe(popup, options, &dom, position) {
                Observation::Reset => {
                    self.scroll(connection, popup, json!(0), wait)?;
                }
                Observation::Advance => {
                    self.scroll(connection, popup, json!("next"), wait)?;
                }
                Observation::Complete => break,
                Observation::Wait => {
                    if !scan.has_options() && discovery.is_expired() && position.is_none() {
                        break;
                    }
                }
            }
            std::thread::sleep(POLL);
        }
        Ok(None)
    }

    fn finish_scan(
        &self,
        connection: &mut CdpConnection,
        scan: &Scan,
        query: &str,
        deadline: Deadline,
    ) -> Result<(Located, i64), AppError> {
        let candidates = scan.matches(query);
        let candidate = match candidates.as_slice() {
            [] => {
                return Err(locate(&scan.options, &self.locator, LocateIntent::Click)
                    .err()
                    .unwrap_or_else(missing_candidate));
            }
            [candidate] => candidate,
            _ => {
                return Err(self.ambiguous(
                    candidates
                        .iter()
                        .filter_map(|entry| entry.option.display_name()),
                ));
            }
        };
        let popup = scan
            .popup
            .ok_or_else(|| AppError::internal("an option has no popup"))?;
        if let Some(top) = candidate.top {
            self.scroll(connection, popup, json!(top), deadline)?;
            let restore = deadline.capped(POLL * 4);
            while restore.remaining() > POLL {
                std::thread::sleep(POLL);
                let dom = frame_dom(connection, self.session, self.combo, restore)?;
                let options = self.options(connection, popup, restore)?;
                let position = self.scroll(connection, popup, Value::Null, restore)?;
                if let Some(node) = candidate.find(options, &dom, position) {
                    return self.pick(&node, popup);
                }
            }
            return Err(missing_candidate());
        }
        self.pick(&scan.options, popup)
    }

    fn pick(&self, options: &AccessibilityNode, popup: i64) -> Result<(Located, i64), AppError> {
        locate(options, &self.locator, LocateIntent::Click)
            .map(|option| (option, popup))
            .map_err(|error| {
                if error.code == ErrorCode::AmbiguousTarget {
                    let query = normalize(self.locator.name.as_deref().unwrap_or_default());
                    self.ambiguous(
                        options
                            .children
                            .iter()
                            .filter(|option| rank(option, &query) == Some(EXACT_NAME))
                            .filter_map(AccessibilityNode::display_name),
                    )
                } else {
                    error
                }
            })
    }

    fn ambiguous<'a>(&self, names: impl Iterator<Item = &'a str>) -> AppError {
        let query = self.locator.name.as_deref().unwrap_or_default();
        let listed = names
            .take(30)
            .map(|name| format!("\"{}\"", crate::escape_line(name, true)))
            .collect::<Vec<_>>();
        AppError::new(
            ErrorCode::AmbiguousTarget,
            format!(
                "multiple options match {query:?}: {}; use a full option name with select. For duplicate names, open the combobox and use tabref snap to get refs for tabref click",
                listed.join(" | ")
            ),
        ).with_details(json!({ "kind": "locate_ambiguous", "candidates": listed }))
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
        to: Value,
        deadline: Deadline,
    ) -> Result<Option<ScrollPosition>, AppError> {
        let reply = call_on_element(
            connection,
            self.session,
            &self.token(backend),
            ElementCall {
                function: SCROLL,
                arguments: vec![to],
            },
            deadline,
        )?;
        Option::<ScrollPosition>::deserialize(reply)
            .map_err(|error| transport_failed("read combobox scroll position", error.to_string()))
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

    pub fn open(
        &self,
        connection: &mut CdpConnection,
        dom: &DomTree,
        known: Option<i64>,
        deadline: Deadline,
    ) -> Result<bool, AppError> {
        if is_collapsed(dom, self.combo) {
            return Ok(false);
        }
        if let Some(popup) = self.popup(dom, known) {
            return self.has_options(connection, popup.backend_id, deadline);
        }
        if known.is_none() && !linked_popup(dom, self.combo).0 {
            for popup in dom
                .nodes
                .iter()
                .filter(|node| is_popup(node) && !self.baseline.contains(&node.backend_id))
            {
                if self.has_options(connection, popup.backend_id, deadline)? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn has_options(
        &self,
        connection: &mut CdpConnection,
        popup: i64,
        deadline: Deadline,
    ) -> Result<bool, AppError> {
        Ok(!self
            .options(connection, popup, deadline)?
            .children
            .is_empty())
    }
}

fn missing_candidate() -> AppError {
    AppError::new(
        ErrorCode::ElementNotFound,
        "the matched combobox option disappeared",
    )
}

fn is_collapsed(dom: &DomTree, combo: &ElementToken) -> bool {
    dom.find_backend(combo.backend_node_id)
        .and_then(|index| dom.node(index))
        .and_then(|node| node.attr("aria-expanded"))
        == Some("false")
}

pub(super) fn initially_open<'d>(dom: &'d DomTree, combo: &ElementToken) -> Option<&'d DomNode> {
    if is_collapsed(dom, combo) {
        return None;
    }
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
