use std::collections::BTreeSet;
use std::time::Duration;

use serde_json::json;
use tabref_cdp::{is_timeout, transport_failed};
use tabref_core::{AccessibilityNode, AppError, Deadline, ErrorCode, NodeIdentity};

use crate::act::element_call::ElementCall;
use crate::act::element_errors::stale;
use crate::act::remote_element::{call_on_element, prove_document};
use crate::tree::ax_nodes::AxNodes;
use crate::tree::web_roles::map_cdp_role;
use crate::{
    ActionNeed, CdpConnection, DomNode, DomTree, ElementToken, LocateIntent, Located, Locator,
    PageSession, SelectOutcome, capture_dom, click, cut_chars, fill, locate, press_key,
    wait_actionable,
};

const POPUP_WAIT: Duration = Duration::from_millis(900);
const POLL: Duration = Duration::from_millis(50);
const POPUP_ROLES: &[&str] = &["listbox", "menu", "grid", "tree"];

const READ_COMBOBOX: &str = r"function() {
  if (!this.isConnected) return null;
  const field = this.matches('input,textarea') ? this :
    this.querySelector('input:not([type=hidden]),textarea');
  const text = this.innerText ?? this.textContent ?? '';
  const autocomplete = this.getAttribute('aria-autocomplete');
  const editable = (field === this && !field.readOnly) || this.isContentEditable;
  return { value: field ? field.value : text.trim(),
    autocomplete: editable && (autocomplete === 'list' || autocomplete === 'both') };
}";

pub(super) fn choose(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    choice: &str,
    deadline: Deadline,
) -> Result<SelectOutcome, AppError> {
    let (previous, autocomplete) = read(connection, session, combo, deadline)?;
    let before = frame_dom(connection, session, combo, deadline)?;
    let baseline = before
        .nodes
        .iter()
        .filter(|node| is_popup(node))
        .map(|node| node.backend_id)
        .collect();
    let locator = Locator {
        role: Some("option".to_owned()),
        name: Some(choice.to_owned()),
        ..Locator::default()
    };
    checked_click(connection, session, combo, "the combobox", deadline)?;
    let search = PopupSearch {
        session,
        combo,
        baseline,
        locator,
    };
    let mut seen = BTreeSet::new();
    let mut tried_query = false;
    let picked = (|| {
        let found = match search.find(connection, &mut seen, deadline) {
            Err(error) if autocomplete && error.code == ErrorCode::ElementNotFound => {
                tried_query = true;
                fill(connection, session, combo, choice, deadline)?;
                search.find(connection, &mut seen, deadline)?
            }
            other => other?,
        };
        let subject = format!("option {:?}", found.name.as_deref().unwrap_or(choice));
        checked_click(connection, session, &found.token, &subject, deadline)?;
        Ok(SelectOutcome {
            label: found.name.unwrap_or_else(|| choice.to_owned()),
            previous: Some(previous.clone()),
            combobox: true,
            ..SelectOutcome::default()
        })
    })();
    if let Err(error) = picked {
        if tried_query
            && read(connection, session, combo, deadline).is_ok_and(|(value, _)| value != previous)
        {
            let _ = fill(connection, session, combo, &previous, deadline);
        }
        if prove_document(connection, session, combo, deadline).is_ok() {
            let _ = press_key(connection, session, "Escape", deadline);
        }
        return Err(with_seen_options(error, seen));
    }
    picked
}

pub(in crate::act) fn read(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    deadline: Deadline,
) -> Result<(String, bool), AppError> {
    let result = call_on_element(
        connection,
        session,
        combo,
        ElementCall {
            function: READ_COMBOBOX,
            arguments: Vec::new(),
        },
        deadline,
    )?;
    let value = result["value"]
        .as_str()
        .ok_or_else(|| stale(combo, "the combobox is no longer in its document"))?;
    Ok((value.to_owned(), result["autocomplete"] == true))
}

fn checked_click(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    subject: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    if let Some(blocked) = wait_actionable(connection, session, token, ActionNeed::Click, deadline)?
    {
        return Err(blocked.error(subject));
    }
    click(connection, session, token, deadline)?;
    Ok(())
}

struct PopupSearch<'a> {
    session: &'a PageSession,
    combo: &'a ElementToken,
    baseline: BTreeSet<i64>,
    locator: Locator,
}

impl PopupSearch<'_> {
    fn find(
        &self,
        connection: &mut CdpConnection,
        seen: &mut BTreeSet<String>,
        deadline: Deadline,
    ) -> Result<Located, AppError> {
        let wait = deadline.capped(POPUP_WAIT.min(deadline.remaining() / 2));
        let mut missing = locate(
            &AccessibilityNode::default(),
            &self.locator,
            LocateIntent::Click,
        );
        loop {
            match self.options(connection, wait) {
                Ok(options) => {
                    seen.extend(options.children.iter().filter_map(|option| {
                        option.display_name().map(|name| cut_chars(name, 60))
                    }));
                    if !options.children.is_empty() {
                        missing = locate(&options, &self.locator, LocateIntent::Click);
                    }
                    if !matches!(&missing, Err(error) if error.code == ErrorCode::ElementNotFound) {
                        return missing;
                    }
                }
                Err(error) if is_timeout(&error) && wait.is_expired() => return missing,
                Err(error) => return Err(error),
            }
            if wait.remaining() <= POLL {
                return missing;
            }
            std::thread::sleep(POLL);
        }
    }

    fn options(
        &self,
        connection: &mut CdpConnection,
        deadline: Deadline,
    ) -> Result<AccessibilityNode, AppError> {
        let dom = frame_dom(connection, self.session, self.combo, deadline)?;
        let Some(popup) = popup(&dom, self.combo, &self.baseline) else {
            return Ok(AccessibilityNode::default());
        };
        let reply = connection.call(
            "Accessibility.queryAXTree",
            json!({ "backendNodeId": popup.backend_id }),
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
                let token = ElementToken {
                    backend_node_id: node.backend_dom_node_id?,
                    ..self.combo.clone()
                };
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
}

fn frame_dom(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    deadline: Deadline,
) -> Result<DomTree, AppError> {
    capture_dom(connection, session, deadline)?
        .into_iter()
        .find(|tree| {
            tree.frame_id == combo.frame_id && tree.find_backend(combo.backend_node_id).is_some()
        })
        .ok_or_else(|| stale(combo, "the combobox is no longer in its document"))
}

fn popup<'a>(
    dom: &'a DomTree,
    combo: &ElementToken,
    before: &BTreeSet<i64>,
) -> Option<&'a DomNode> {
    let node = dom.node(dom.find_backend(combo.backend_node_id)?)?;
    let mut declared = false;
    for id in ["aria-controls", "aria-owns"]
        .into_iter()
        .filter_map(|attr| node.attr(attr))
        .flat_map(str::split_whitespace)
    {
        if let Some(target) = dom.nodes.iter().find(|node| node.attr("id") == Some(id)) {
            declared = true;
            if target.is_shown() {
                return Some(target);
            }
        }
    }
    if declared {
        return None;
    }
    let mut appeared = dom
        .nodes
        .iter()
        .filter(|node| is_popup(node) && !before.contains(&node.backend_id));
    let first = appeared.next()?;
    appeared.next().is_none().then_some(first)
}

fn is_popup(node: &DomNode) -> bool {
    node.is_shown()
        && node
            .attr("role")
            .and_then(|roles| roles.split_whitespace().next())
            .is_some_and(|role| POPUP_ROLES.contains(&role))
}

fn with_seen_options(mut error: AppError, seen: BTreeSet<String>) -> AppError {
    if error.code != ErrorCode::ElementNotFound {
        return error;
    }
    let shown: Vec<String> = seen.iter().take(30).cloned().collect();
    if shown.is_empty() {
        error.message.push_str("; no popup options appeared");
    } else {
        error
            .message
            .push_str(&format!("; options: {}", shown.join(" | ")));
        if seen.len() > shown.len() {
            error
                .message
                .push_str(&format!(" … and {} more", seen.len() - shown.len()));
        }
    }
    if let Some(details) = &mut error.details {
        details["options"] = json!(shown);
        details["options_total"] = json!(seen.len());
    }
    error
}
