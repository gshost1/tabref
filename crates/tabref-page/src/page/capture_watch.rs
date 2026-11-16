//! Tells whether a page is still the page a capture read. Before the
//! capture, one evaluation starts `MutationObserver`s on the document, every
//! open shadow root and every same-origin frame document, and records the
//! roots it found and a fingerprint of the state no mutation shows: the
//! address, scroll position, viewport and document size, the focused
//! element, form values and open popovers. Later, one more evaluation says
//! whether any of that changed. The state hangs off a registered symbol of
//! `window`, apart from the action watch's, and a document that replaced the
//! watched one has none, which is how a navigation shows. A page with a
//! frame or embedded object whose document the script cannot read is blind:
//! it never counts as unchanged.

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::evaluate::evaluate_quietly;
use crate::{CdpConnection, PageSession};

const SCAN: &str = r"const scan = () => {
    const roots = [];
    let blind = false;
    let state = '';
    const walk = root => {
      roots.push(root);
      for (const el of root.querySelectorAll('*')) {
        if (el.shadowRoot) walk(el.shadowRoot);
        const tag = el.localName;
        if (tag === 'iframe' || tag === 'frame' || tag === 'object' || tag === 'embed') {
          let inner = null;
          try { inner = el.contentDocument; } catch (error) { inner = null; }
          if (inner) walk(inner); else blind = true;
        } else if (tag === 'input' || tag === 'textarea' || tag === 'select') {
          state += '\u0001' + el.value + '\u0002' + (el.checked ? 1 : 0) + '\u0002' + (el.selectedIndex ?? '');
        }
        if (el.hasAttribute('popover')) {
          try { state += '\u0003' + (el.matches(':popover-open') ? 1 : 0); } catch (error) { blind = true; }
        }
      }
    };
    walk(document);
    let active = document.activeElement;
    while (active && active.shadowRoot && active.shadowRoot.activeElement) active = active.shadowRoot.activeElement;
    const box = document.documentElement;
    const place = [location.href, scrollX, scrollY, innerWidth, innerHeight, devicePixelRatio,
      box ? box.scrollWidth : 0, box ? box.scrollHeight : 0].join('\u0004');
    return { roots, blind, active, print: place + '\u0005' + state };
  };";

const ARM: &str = r"(() => {
  KEY_AND_SCAN
  const old = window[key];
  if (old) old.observers.forEach(observer => observer.disconnect());
  const now = scan();
  const watch = { token: TOKEN, doc: document, dirty: false, observers: [], roots: now.roots,
    active: now.active, print: now.print, blind: now.blind };
  const stop = () => {
    watch.dirty = true;
    watch.observers.forEach(observer => observer.disconnect());
  };
  for (const root of now.roots) {
    const observer = new MutationObserver(stop);
    observer.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    watch.observers.push(observer);
  }
  window[key] = watch;
  return { blind: now.blind };
})()";

const CHECK: &str = r"(() => {
  KEY_AND_SCAN
  const watch = window[key];
  if (!watch || watch.token !== TOKEN || watch.doc !== document) return 'gone';
  if (watch.observers.some(observer => observer.takeRecords().length > 0)) watch.dirty = true;
  if (watch.dirty) return 'mutated';
  if (watch.blind) return 'blind';
  const now = scan();
  if (now.blind) return 'blind';
  if (now.roots.length !== watch.roots.length || now.roots.some((root, i) => root !== watch.roots[i])) return 'roots';
  if (now.active !== watch.active) return 'focus';
  if (now.print !== watch.print) return 'state';
  return 'same';
})()";

/// A watch armed in the page before a capture: its token, and whether the
/// script could see every document and root (`blind` when not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureWatch {
    pub token: String,
    pub blind: bool,
}

/// Arms a watch named `token` in the page, replacing any earlier one.
pub fn arm_capture_watch(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &str,
    deadline: Deadline,
) -> Result<CaptureWatch, AppError> {
    let reply = evaluate_quietly(connection, session, &script(ARM, token), deadline)?;
    Ok(CaptureWatch {
        token: token.to_owned(),
        blind: reply.get("blind").and_then(Value::as_bool).unwrap_or(true),
    })
}

/// What the page says about `watch` now: `same` when nothing it watches
/// changed, else why not (`gone`, `mutated`, `blind`, `roots`, `focus`,
/// `state`), or `failed` when the page could not be asked.
pub fn check_capture_watch(
    connection: &mut CdpConnection,
    session: &PageSession,
    watch: &CaptureWatch,
    deadline: Deadline,
) -> String {
    match evaluate_quietly(connection, session, &script(CHECK, &watch.token), deadline) {
        Ok(Value::String(verdict)) => verdict,
        Ok(_) | Err(_) => "failed".to_owned(),
    }
}

/// Whether a `DOMSnapshot.captureSnapshot` reply holds a node of a closed
/// shadow root, which no page script can watch.
pub fn has_closed_shadow_root(capture: &Value) -> bool {
    let strings = capture.get("strings").and_then(Value::as_array);
    let closed = |index: &Value| {
        index
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| strings?.get(index))
            .and_then(Value::as_str)
            == Some("closed")
    };
    capture
        .get("documents")
        .and_then(Value::as_array)
        .is_some_and(|documents| {
            documents
                .iter()
                .filter_map(|document| document.pointer("/nodes/shadowRootType/value"))
                .filter_map(Value::as_array)
                .any(|values| values.iter().any(closed))
        })
}

fn script(template: &str, token: &str) -> String {
    let head = format!("const key = Symbol.for('tabref.capture-watch');\n  {SCAN}");
    template
        .replace("KEY_AND_SCAN", &head)
        .replace("TOKEN", &json!(token).to_string())
}

#[cfg(test)]
#[path = "capture_watch_tests.rs"]
mod tests;
