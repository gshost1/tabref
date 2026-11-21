use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use crate::act::element_errors::script_failed;
use crate::act::remote_element::with_element;
use crate::{CdpConnection, ElementToken, PageSession};

/// What a scroll moves: the page, or the container that holds an element.
/// Each script a scroll runs gets the site as `this`: the page's `window`,
/// or the element, whose own document and window the script then uses.
#[derive(Debug, Clone, Copy)]
pub enum ScrollSite<'a> {
    Page,
    Element(&'a ElementToken),
}

/// A script for a [`ScrollSite`]: the function's source and the JSON
/// arguments to pass it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SiteScript<'s> {
    pub(crate) function: &'s str,
    pub(crate) arguments: &'s [Value],
}

impl ScrollSite<'_> {
    /// Settles a loop step, watching a quiet window even at a stationary end.
    pub fn scroll_loop_step(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        delta: (f64, f64),
        deadline: Deadline,
    ) -> Result<crate::ScrollOutcome, AppError> {
        crate::settle::scroll_settle::settle_step(
            connection,
            session,
            self,
            (delta, true),
            deadline,
        )
    }

    /// Checks rendered text inside the same container a scroll would move.
    pub fn contains_visible_text(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        text: &str,
        deadline: Deadline,
    ) -> Result<bool, AppError> {
        self.call(
            connection,
            session,
            SiteScript {
                function: VISIBLE_TEXT,
                arguments: &[json!(text)],
            },
            deadline,
        )
        .map(|value| value.as_bool().unwrap_or(false))
    }

    /// Runs `function` with the site as `this` and returns its JSON value,
    /// awaiting a promise it returns. The script is tabref's own, never a
    /// user gesture.
    pub(crate) fn call(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        script: SiteScript<'_>,
        deadline: Deadline,
    ) -> Result<Value, AppError> {
        let mut reply = self.run(connection, session, script, deadline)?;
        Ok(reply
            .pointer_mut("/result/value")
            .map_or(Value::Null, Value::take))
    }

    fn run(
        self,
        connection: &mut CdpConnection,
        session: &PageSession,
        script: SiteScript<'_>,
        deadline: Deadline,
    ) -> Result<Value, AppError> {
        let SiteScript {
            function,
            arguments,
        } = script;
        let reply = match self {
            Self::Page => {
                let listed: Vec<String> = arguments.iter().map(Value::to_string).collect();
                let expression = format!("({function}).call(window, {})", listed.join(", "));
                connection.call(
                    "Runtime.evaluate",
                    json!({
                        "expression": expression,
                        "returnByValue": true,
                        "awaitPromise": true,
                    }),
                    Some(&session.session_id),
                    deadline,
                )?
            }
            Self::Element(token) => with_element(
                connection,
                session,
                token,
                deadline,
                |connection, object| {
                    let arguments: Vec<Value> = arguments
                        .iter()
                        .map(|value| json!({ "value": value }))
                        .collect();
                    connection.call(
                        "Runtime.callFunctionOn",
                        json!({
                        "objectId": object,
                        "functionDeclaration": function,
                        "arguments": arguments,
                        "returnByValue": true,
                        "awaitPromise": true,
                        }),
                        Some(&session.session_id),
                        deadline,
                    )
                },
            )?,
        };
        if let Some(exception) = reply.get("exceptionDetails") {
            return Err(script_failed(exception));
        }
        Ok(reply)
    }
}

const VISIBLE_TEXT: &str = r"function(needle) {
  const anchor = this && this.nodeType === 1 ? this : null;
  const doc = anchor ? anchor.ownerDocument : document;
  const win = doc.defaultView;
  const parent = node => node.parentElement || (node.getRootNode() || {}).host;
  const scrolls = el => {
    const style = win.getComputedStyle(el);
    return /(auto|scroll|overlay)/.test(style.overflowY + ' ' + style.overflowX)
      && (el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth);
  };
  let root = anchor;
  while (root && !scrolls(root)) root = parent(root);
  root = root || doc;
  const fold = s => Array.from(s.replace(/\p{White_Space}+/gu,' ').replace(/^ | $/g,''), c => Array.from(c.toLowerCase())[0]).join('');
  needle = fold(needle);
  if (!needle) return false;
  let matched = false;
  const skipped = new Set(['HEAD','SCRIPT','STYLE','NOSCRIPT','TEMPLATE','TEXTAREA']);
  function visibleText(node) {
    const element = node.parentElement || node.getRootNode().host;
    if (!element || win.getComputedStyle(element).visibility !== 'visible') return '';
    for (let ancestor = element; ancestor; ancestor = parent(ancestor)) {
      const style = win.getComputedStyle(ancestor);
      if (style.display === 'none' || Number(style.opacity) === 0) return '';
      const clip = style.clip.match(/^rect\((.*)\)$/);
      if (clip) {
        const edges = clip[1].split(/[,\s]+/).map(parseFloat);
        if (edges[2] <= edges[0] || edges[1] <= edges[3]) return '';
      }
      if (style.clipPath === 'inset(50%)') return '';
      const rect = ancestor.getBoundingClientRect();
      if (/(hidden|clip)/.test(style.overflowX + ' ' + style.overflowY) && rect.width <= 1 && rect.height <= 1) return '';
    }
    const range = doc.createRange();
    range.selectNodeContents(node);
    return Array.from(range.getClientRects()).some(rect => rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.left < win.innerWidth)
      ? node.textContent : '';
  }
  function scan(node) {
    if (node.nodeType === 3) return visibleText(node);
    if (node.nodeType === 1 && skipped.has(node.tagName)) return '';
    let text = '';
    for (const child of (node.shadowRoot || node).childNodes) {
      if (node.nodeName === 'DETAILS' && !node.open && child.nodeName !== 'SUMMARY') continue;
      const block = /^(ADDRESS|ARTICLE|ASIDE|BLOCKQUOTE|DIV|DL|DT|DD|FIELDSET|FIGCAPTION|FIGURE|FOOTER|FORM|H[1-6]|HEADER|HR|LI|MAIN|NAV|OL|P|PRE|SECTION|TABLE|TD|TH|TR|UL)$/.test(child.nodeName);
      text += (block ? ' ' : '') + scan(child) + (block ? ' ' : '');
    }
    if (fold(text).includes(needle)) matched = true;
    return text;
  }
  scan(root);
  return matched;
}";
