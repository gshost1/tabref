//! Tells whether a page is still the page a capture read. Before the
//! capture, one evaluation waits for the page's main thread to go idle (at
//! most [`IDLE_WAIT_MS`]), so work the page had already queued, such as a
//! fetch callback that fills a box, runs before the watch starts rather
//! than just after it (a hidden tab, whose idle callbacks and timers a
//! browser may hold back, arms at once), and then starts `MutationObserver`s on the document, every
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
//!
//! Every element's visibility is checked; pages above [`MAX_ELEMENTS`] are
//! blind. Form validity and finished fill animations are watched too, since
//! they can change rendering without mutations. Upgraded custom elements
//! are blind: ElementInternals can change accessibility and custom states
//! without observable mutations, including scoped registries. Manual slots,
//! until-found content and container queries are blind. Opaque stylesheet
//! identity, href, disabled and media state are recorded, but unreadable
//! rules require fail-closed CDP text classification before approval,
//! including imports inside readable sheets. Other stylesheet errors and oversized
//! form or stylesheet state are blind rather than sampled.
//! Supported state pseudo-classes are fingerprinted by matched node identity.

use serde_json::{Value, json};
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline};

use super::evaluate::evaluate_quietly;
use crate::{CdpConnection, PageSession};

const MAX_ELEMENTS: usize = 30_000;

#[path = "capture_watch_css.rs"]
mod css;
#[path = "capture_watch_opaque.rs"]
mod opaque;
#[path = "capture_watch_trace.rs"]
mod trace;

const SCAN: &str = r"const scan = (approved = []) => {
    const opaque = [];
    const roots = [];
    const sheetObjects = [];
    const animations = [];
    const states = [];
    let blind = false;
    let why = '';
    const fail = reason => { blind = true; why = reason; };
    let state = '';
    let total = 0;
    let visible = 0;
    let visibility = 0;
    let forms = 0;
    let styleSize = 0;
    let ruleCount = 0;
    const visibilityOptions = { opacityProperty: true, visibilityProperty: true };
    const frames = 'iframe, frame, object, embed';
    const moving = root => {
      try {
        const entries = root.getAnimations().map(animation => ({ animation,
          playState: animation.playState, currentTime: animation.currentTime }));
        animations.push(...entries);
        const active = entries.find(entry => entry.animation.pending || entry.playState === 'running');
        if (active) fail('animation:' + (active.animation.animationName ?? active.animation.constructor.name));
        return !!active;
      } catch (error) { fail('animation:' + error.name); return true; }
    };
    const unreadable = root => {
      for (const el of root.querySelectorAll(frames)) {
        let inner = null;
        try { inner = el.contentDocument; } catch (error) { inner = null; }
        if (!inner) { fail('frame:' + el.localName + ':' + (el.getAttribute('src') ?? el.getAttribute('data') ?? '')); return true; }
      }
      return false;
    };
    const styles = (sheets, realm) => {
      for (const sheet of sheets) {
        sheetObjects.push(sheet, sheet.ownerNode, sheet.ownerRule);
        state += '\u0008' + JSON.stringify([sheet.href, sheet.disabled, sheet.media.mediaText]);
        if (sheet.media.mediaText) state += realm.matchMedia(sheet.media.mediaText).matches;
        const rules = list => {
          for (const rule of list) {
            if (++ruleCount > 10000) { fail('rule-count'); return; }
            const text = rule.cssText;
            if (text.startsWith('@container')) { fail('container'); return; }
            styleSize += text.length;
            if (styleSize > 100_000) { fail('style-size'); return; }
            state += '\u0009' + text;
            if (rule.media) state += realm.matchMedia(rule.media.mediaText).matches;
            if (rule.styleSheet) styles([rule.styleSheet], realm);
            else if (rule.cssRules) rules(rule.cssRules);
            if (blind) return;
          }
        };
        try { rules(sheet.cssRules); } catch (error) {
          if (error.name === 'SecurityError' && realm === window) {
            state += ':opaque';
            if (!approved.some(entry => entry.sheet === sheet && entry.href === sheet.href)) opaque.push({sheet, href: sheet.href});
            if (opaque.length > 128) blind = true;
          } else fail('stylesheet:' + error.name);
        }
        if (blind) return;
      }
    };
    const walk = root => {
      roots.push(root);
      if (root.slotAssignment === 'manual') { fail('manual-slot'); return; }
      if (moving(root) || unreadable(root)) return;
      const elements = root.querySelectorAll('*');
      total += elements.length;
      if (total > MAX_ELEMENTS) { fail('element-count'); return; }
      state += '\u0006' + (root.styleSheets?.length ?? 0) + ':' + (root.adoptedStyleSheets?.length ?? 0);
      const doc = root.nodeType === 9 ? root : root.ownerDocument;
      const realm = doc.defaultView;
      const selectors = [':hover', ':active', ':fullscreen', ':focus-visible', ':focus-within',
        ':user-invalid', ':user-valid', ':autofill', ':indeterminate', ':checked', ':modal', ':open',
        ':popover-open', ':target', ':target-within', ':placeholder-shown', ':blank', ':playing',
        ':paused', ':seeking', ':buffering', ':stalled', ':muted', ':volume-locked', ':picture-in-picture']
        .filter(selector => { try { return realm.CSS.supports('selector(' + selector + ')'); } catch { return false; } });
      if (selectors.length) {
        for (const el of root.querySelectorAll(selectors.join(','))) {
          for (let i = 0; i < selectors.length; i++) {
            try { if (el.matches(selectors[i])) { state += '\u000a' + i; states.push(el); } } catch {}
          }
        }
      }
      for (const query of ['(prefers-color-scheme: dark)', '(prefers-color-scheme: light)',
        '(prefers-reduced-motion: reduce)', '(prefers-reduced-motion: no-preference)',
        '(prefers-contrast: more)', '(prefers-contrast: less)', '(prefers-contrast: custom)',
        '(prefers-contrast: no-preference)', '(forced-colors: active)', '(forced-colors: none)',
        '(orientation: portrait)', '(orientation: landscape)', 'screen', 'print', 'all']) state += ':' + realm.matchMedia(query).matches;
      styles([...(root.styleSheets ?? []), ...(root.adoptedStyleSheets ?? [])], doc.defaultView);
      if (blind) return;
      state += ':' + doc.defaultView.scrollX + ':' + doc.defaultView.scrollY
        + ':' + doc.defaultView.innerWidth + ':' + doc.defaultView.innerHeight;
      for (let index = 0; index < elements.length; index++) {
        if (blind) return;
        const el = elements[index];
        if (el.checkVisibility(visibilityOptions)) {
          visible += 1;
          visibility = (Math.imul(31, visibility) + index + 1) | 0;
        }
        if (el.shadowRoot) walk(el.shadowRoot);
        const tag = el.localName;
        if ((tag.includes('-') && el.matches(':defined')) || el.getAttribute('hidden') === 'until-found') { fail('custom-or-until-found'); return; }
        if (tag === 'iframe' || tag === 'frame' || tag === 'object' || tag === 'embed') {
          let inner = null;
          try { inner = el.contentDocument; } catch (error) { inner = null; }
          if (inner) walk(inner); else blind = true;
        } else if (tag === 'input' || tag === 'textarea' || tag === 'select') {
          forms += 1;
          if (forms > 1_000 || el.value.length > 100_000
            || (el.options && el.options.length > 1_000)) { blind = true; return; }
          state += '\u0001' + el.value + '\u0002' + (el.checked ? 1 : 0) + (el.indeterminate ? 1 : 0)
            + '\u0002' + (el.selectedIndex ?? '');
          state += '\u0002' + JSON.stringify([el.validity.valid, el.validationMessage]);
          if (el.multiple && el.selectedOptions) state += '\u0002' + [...el.selectedOptions].map(option => option.index).join(',');
          if (state.length > 100_000) { blind = true; return; }
        }
        if (el.hasAttribute('popover')) {
          try { state += '\u0003' + (el.matches(':popover-open') ? 1 : 0); } catch (error) { blind = true; }
        }
      }
    };
    walk(document);
    if (blind) return { roots, blind, why, active: null, print: '' };
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
    return { roots, sheets: sheetObjects, animations, states, opaque, blind, active, print: place + '\u0005' + state + '\u0007' + visible + ':' + visibility };
  };";

/// How long the arm script waits for an idle main thread before it arms
/// anyway. Waiting never makes a capture staler: the watch is still armed
/// before the capture starts, and only later changes are what it watches.
pub const IDLE_WAIT_MS: u64 = 50;

const ARM: &str = r"await new Promise(resolve => { const arm = () => resolve((() => {
  KEY_AND_SCAN
  const old = window[key];
  if (old) old.stop();
  const pending = window[Symbol.for('tabref.capture-opaque')];
  const opaque = pending?.token === TOKEN ? pending.approved ?? [] : [];
  delete window[Symbol.for('tabref.capture-opaque')];
  const now = scan(opaque);
  if (now.blind) { delete window[key]; return { blind: true, why: now.why }; }
  if (now.opaque.length) {
    window[Symbol.for('tabref.capture-opaque')] = {token: TOKEN, entries: now.opaque};
    delete window[key];
    return {blind: true, opaque: now.opaque.map(entry => entry.href)};
  }
  const watch = { token: TOKEN, doc: document, dirty: false, observers: [], roots: now.roots,
    sheets: now.sheets, animations: now.animations, states: now.states, active: now.active, print: now.print, blind: now.blind, opaque };
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
        try { Object.defineProperty(prototype, method, descriptor); } catch {}
      }
    });
  };
  try {
    for (const root of now.roots) {
      if (root.nodeType !== 9) continue;
      const realm = root.defaultView;
      wrap(realm.Element.prototype, 'attachShadow');
      wrap(realm.HTMLElement.prototype, 'attachInternals');
      wrap(realm.HTMLSlotElement.prototype, 'assign');
      wrap(realm.CSS, 'registerProperty');
      for (const method of ['add', 'delete', 'clear']) wrap(realm.FontFaceSet.prototype, method);
      const fonts = realm.document.fonts;
      fonts.addEventListener('loadingdone', watch.stop);
      restore.push(() => fonts.removeEventListener('loadingdone', watch.stop));
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
})());
  if (document.visibilityState === 'hidden') arm();
  else (window.requestIdleCallback || (run => setTimeout(run, 0)))(arm, { timeout: IDLE_MS });
})";

const CHECK: &str = r"(() => {
  KEY_AND_SCAN
  const watch = window[key];
  if (!watch || watch.token !== TOKEN || watch.doc !== document) return 'gone';
  if (watch.observers.some(observer => observer.takeRecords().length > 0)) watch.stop();
  if (watch.dirty) return 'mutated';
  if (watch.blind) return 'blind';
  const now = scan(watch.opaque);
  if (now.blind || now.opaque.length) return 'blind';
  if (now.roots.length !== watch.roots.length || now.roots.some((root, i) => root !== watch.roots[i])) return 'roots';
  if (now.sheets.length !== watch.sheets.length || now.sheets.some((sheet, i) => sheet !== watch.sheets[i])) return 'state';
  if (now.states.length !== watch.states.length || now.states.some((node, i) => node !== watch.states[i])) return 'state';
  if (now.animations.length !== watch.animations.length || now.animations.some((entry, i) => {
    const old = watch.animations[i];
    return entry.animation !== old.animation || entry.playState !== old.playState || entry.currentTime !== old.currentTime;
  })) return 'state';
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
    let mut reply = evaluate_quietly(connection, session, &script(ARM, token), deadline)
        .inspect_err(|error| {
            trace::record(
                "arm.error",
                || json!({"token":token,"step":"initial","error":error.to_string()}),
            );
        })?;
    trace::record("arm.scan", || json!({"token":token,"reply":reply}));
    if let Some(urls) = reply.get("opaque").and_then(Value::as_array) {
        match opaque::prepare(connection, session, token, urls, deadline) {
            Ok(()) => {
                let rearm = ARM.replace("if (document.visibilityState === 'hidden')", "if (true)");
                reply = evaluate_quietly(connection, session, &script(&rearm, token), deadline)
                    .inspect_err(|error| {
                        trace::record(
                            "arm.error",
                            || json!({"token":token,"step":"approved","error":error.to_string()}),
                        );
                    })?;
                trace::record("arm.approved", || json!({"token":token,"reply":reply}));
            }
            Err(error) => trace::record(
                "opaque.error",
                || json!({"token":token,"error":error.to_string()}),
            ),
        }
    }
    Ok(CaptureWatch {
        token: token.to_owned(),
        blind: reply.get("blind").and_then(Value::as_bool).unwrap_or(true),
    })
}

/// What the page says about `watch` now: `same` when nothing it watches
/// changed, else why not (`gone`, `mutated`, `blind`, `roots`, `focus`,
/// `state`), `timed_out` when the check exceeded its deadline, or
/// `failed` when the page could not be asked for another reason.
pub fn check_capture_watch(
    connection: &mut CdpConnection,
    session: &PageSession,
    watch: &CaptureWatch,
    deadline: Deadline,
) -> String {
    let verdict =
        match evaluate_quietly(connection, session, &script(CHECK, &watch.token), deadline) {
            Ok(Value::String(verdict)) => verdict,
            Err(error) if is_timeout(&error) => "timed_out".to_owned(),
            Ok(_) | Err(_) => "failed".to_owned(),
        };
    trace::record("check", || json!({"token":watch.token,"verdict":verdict}));
    verdict
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
        .replace("IDLE_MS", &IDLE_WAIT_MS.to_string())
}

#[cfg(test)]
#[path = "capture_watch_tests.rs"]
mod tests;
