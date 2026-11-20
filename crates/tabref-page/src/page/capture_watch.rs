//! Tells whether a page is still the page a capture read. Before the
//! capture, one evaluation starts `MutationObserver`s on the document, every
//! open shadow root and every same-origin frame document, and records the
//! roots it found and a fingerprint of the state no mutation shows: the
//! address, scroll position, viewport and document size, the focused
//! element, form values, open popovers, stylesheet counts and computed
//! visibility. Later, one more evaluation says
//! whether any of that changed. The state hangs off a registered symbol of
//! `window`, apart from the action watch's, and a document that replaced the
//! watched one has none, which is how a navigation shows. A page with a
//! frame or embedded object whose document the script cannot read, or with
//! a CSS animation or transition running or pending in any document or
//! shadow root (each root asked on its own: in Chrome 156 a document's list
//! leaves out its shadow trees and frames), is blind: it never counts as unchanged, and the scan stops as
//! soon as it knows. Form state includes a checkbox's `indeterminate` and a
//! multi-select's whole selection; focus is followed into open shadow roots
//! and same-origin frames.

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::evaluate::evaluate_quietly;
use crate::{CdpConnection, PageSession};

const MAX_ELEMENTS: usize = 1_000;

const SCAN: &str = r"const scan = () => {
    const roots = [];
    let blind = false;
    let state = '';
    let total = 0;
    let visible = 0;
    let visibility = 0;
    const visibilityOptions = { opacityProperty: true, visibilityProperty: true };
    const frames = 'iframe, frame, object, embed';
    const moving = root => {
      try {
        return root.getAnimations().some(animation => animation.pending || animation.playState === 'running');
      } catch (error) { return true; }
    };
    const unreadable = root => {
      for (const el of root.querySelectorAll(frames)) {
        let inner = null;
        try { inner = el.contentDocument; } catch (error) { inner = null; }
        if (!inner) return true;
      }
      return false;
    };
    const walk = root => {
      roots.push(root);
      if (root.nodeType === 9 && root.getElementsByTagName('*').length > MAX_ELEMENTS - total) {
        blind = true;
        return;
      }
      if (moving(root) || unreadable(root)) { blind = true; return; }
      const elements = root.querySelectorAll('*');
      total += elements.length;
      if (total > MAX_ELEMENTS) { blind = true; return; }
      state += '\u0006' + (root.styleSheets?.length ?? 0) + ':' + (root.adoptedStyleSheets?.length ?? 0);
      for (let index = 0; index < elements.length; index++) {
        if (blind) return;
        const el = elements[index];
        if (el.checkVisibility(visibilityOptions)) {
          visible += 1;
          visibility = (Math.imul(31, visibility) + index + 1) | 0;
        }
        if (el.shadowRoot) walk(el.shadowRoot);
        const tag = el.localName;
        if (tag === 'iframe' || tag === 'frame' || tag === 'object' || tag === 'embed') {
          let inner = null;
          try { inner = el.contentDocument; } catch (error) { inner = null; }
          if (inner) walk(inner); else blind = true;
        } else if (tag === 'input' || tag === 'textarea' || tag === 'select') {
          state += '\u0001' + el.value + '\u0002' + (el.checked ? 1 : 0) + (el.indeterminate ? 1 : 0)
            + '\u0002' + (el.selectedIndex ?? '');
          if (el.multiple && el.selectedOptions) state += '\u0002' + [...el.selectedOptions].map(option => option.index).join(',');
        }
        if (el.hasAttribute('popover')) {
          try { state += '\u0003' + (el.matches(':popover-open') ? 1 : 0); } catch (error) { blind = true; }
        }
      }
    };
    walk(document);
    if (blind) return { roots, blind, active: null, print: '' };
    let active = document.activeElement;
    for (;;) {
      if (active && active.shadowRoot && active.shadowRoot.activeElement) { active = active.shadowRoot.activeElement; continue; }
      let inner = null;
      try { inner = active && active.contentDocument; } catch (error) { inner = null; }
      if (inner && inner.activeElement) { active = inner.activeElement; continue; }
      break;
    }
    const box = document.documentElement;
    const place = [location.href, scrollX, scrollY, innerWidth, innerHeight, devicePixelRatio,
      box ? box.scrollWidth : 0, box ? box.scrollHeight : 0].join('\u0004');
    return { roots, blind, active, print: place + '\u0005' + state + '\u0007' + visible + ':' + visibility };
  };";

const ARM: &str = r"(() => {
  KEY_AND_SCAN
  const old = window[key];
  if (old) old.stop();
  const now = scan();
  if (now.blind) { delete window[key]; return { blind: true }; }
  const watch = { token: TOKEN, doc: document, dirty: false, observers: [], roots: now.roots,
    active: now.active, print: now.print, blind: now.blind };
  const restore = [];
  watch.stop = () => {
    if (watch.dirty) return;
    watch.dirty = true;
    watch.observers.forEach(observer => observer.disconnect());
    restore.forEach(undo => undo());
  };
  const wrap = (prototype, method) => {
    const descriptor = Object.getOwnPropertyDescriptor(prototype, method);
    if (!descriptor || typeof descriptor.value !== 'function') throw new Error(method);
    const wrapped = function(...args) {
      watch.stop();
      return Reflect.apply(descriptor.value, this, args);
    };
    Object.defineProperty(prototype, method, { ...descriptor, value: wrapped });
    restore.push(() => {
      if (Object.getOwnPropertyDescriptor(prototype, method)?.value === wrapped) {
        Object.defineProperty(prototype, method, descriptor);
      }
    });
  };
  try {
    for (const root of now.roots) {
      if (root.nodeType !== 9) continue;
      const realm = root.defaultView;
      wrap(realm.Element.prototype, 'attachShadow');
      wrap(realm.CustomElementRegistry.prototype, 'define');
      wrap(realm.CustomElementRegistry.prototype, 'upgrade');
    }
  } catch (error) {
    watch.stop();
    delete window[key];
    return { blind: true };
  }
  for (const root of now.roots) {
    const observer = new MutationObserver(watch.stop);
    observer.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    watch.observers.push(observer);
  }
  window[key] = watch;
  setTimeout(watch.stop, WATCH_MS);
  return { blind: now.blind };
})()";

const CHECK: &str = r"(() => {
  KEY_AND_SCAN
  const watch = window[key];
  if (!watch || watch.token !== TOKEN || watch.doc !== document) return 'gone';
  if (watch.observers.some(observer => observer.takeRecords().length > 0)) watch.stop();
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
        .replace("MAX_ELEMENTS", &MAX_ELEMENTS.to_string())
        .replace("WATCH_MS", "5000")
        .replace("TOKEN", &json!(token).to_string())
}

#[cfg(test)]
#[path = "capture_watch_tests.rs"]
mod tests;
