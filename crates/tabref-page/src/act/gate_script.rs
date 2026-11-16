//! The one in-page source of the action gate's rules. The gate before an
//! action and a wait for an element's state both run it, so an element a
//! wait finds `visible` or `enabled` is one the next action accepts.

/// `function(node, need, controls)`: whether `node` (a text node stands for
/// its parent element) can take an action that needs `need`. Returns
/// `{ ready: true }`, or `{ blocker, detail }` with blocker `detached`,
/// `hidden`, `unpressable`, `zero`, `disabled` or `readonly`.
pub(crate) const GATE: &str = r"function(node, need, controls) {
  const el = node.nodeType === 1 ? node : node.parentElement;
  if (!el || !el.isConnected) return { blocker: 'detached' };
  const up = node => node.parentElement || (node.parentNode && node.parentNode.host) || null;
  const view = el.ownerDocument.defaultView;
  for (let node = el; node; node = up(node)) {
    if (node.hasAttribute('inert')) return { blocker: 'hidden', detail: 'inside an inert section' };
    if (node.getAttribute('aria-hidden') === 'true') return { blocker: 'hidden', detail: 'aria-hidden' };
  }
  if (need.box && el.localName === 'area') return { blocker: 'unpressable', detail: 'image-map area' };
  const area = boxes => Array.from(boxes).some(box => box.width > 0 && box.height > 0);
  const textArea = text => { const range = text.ownerDocument.createRange(); range.selectNodeContents(text); return area(range.getClientRects()); };
  const children = node => {
    const own = node.localName === 'slot' ? Array.from(node.assignedNodes({ flatten: true })) : [];
    const shadow = node.shadowRoot ? Array.from(node.shadowRoot.childNodes) : [];
    return own.concat(shadow, Array.from(node.childNodes));
  };
  const drawn = node => {
    const style = view.getComputedStyle(node);
    if (style.display === 'contents') return children(node).some(child => child.nodeType === 1 ? drawn(child) : child.nodeType === 3 && textArea(child));
    if (node.checkVisibility && !node.checkVisibility()) return false;
    if (style.visibility === 'hidden' || style.visibility === 'collapse') return false;
    return !need.box || area(node.getClientRects());
  };
  const style = view.getComputedStyle(el);
  if (style.display === 'contents') {
    if (!drawn(el)) return { blocker: 'hidden', detail: 'display:contents with nothing shown' };
  } else {
    if (el.checkVisibility && !el.checkVisibility()) {
      let detail = 'not rendered';
      if (el.closest('details:not([open])')) detail = 'inside a closed section';
      for (let node = el; node; node = up(node)) {
        if (view.getComputedStyle(node).display === 'none') { detail = 'display:none'; break; }
      }
      return { blocker: 'hidden', detail };
    }
    if (style.display === 'none') return { blocker: 'hidden', detail: 'display:none' };
    if (style.visibility === 'hidden' || style.visibility === 'collapse') return { blocker: 'hidden', detail: 'visibility:hidden' };
    if (need.box && !area(el.getClientRects())) {
      const alone = label => Array.from(label.querySelectorAll(controls)).every(inner => inner === el);
      const labelled = need.label && Array.from(el.labels || []).some(label => area(label.getClientRects()) && alone(label));
      if (!labelled) return { blocker: 'zero', detail: 'zero size' };
    }
  }
  if (need.enabled) {
    const owner = el.closest('button, input, select, textarea, option, optgroup') || el;
    if (owner.matches(':disabled')) {
      const detail = owner.hasAttribute('disabled') ? 'disabled'
        : owner.closest('fieldset[disabled]') ? 'inside a disabled fieldset' : 'inside a disabled group';
      return { blocker: 'disabled', detail };
    }
    for (let node = el; node; node = up(node)) {
      const aria = node.getAttribute('aria-disabled');
      if (aria === 'true') return { blocker: 'disabled', detail: 'aria-disabled' };
      if (aria === 'false') break;
    }
  }
  if (need.editable) {
    const field = el.localName === 'input' || el.localName === 'textarea';
    if (field && el.readOnly) return { blocker: 'readonly', detail: 'read-only' };
    if (el.getAttribute('aria-readonly') === 'true') return { blocker: 'readonly', detail: 'aria-readonly' };
  }
  return { ready: true };
}";
