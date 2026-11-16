//! The page scripts behind the change watch. `SCAN` finds the open shadow
//! roots and same-origin frame documents of the page, nested either way,
//! and counts the shown frames from other sites, whose documents a page
//! script cannot read. `TEXT` is the rendered text of the document, then of
//! each shown shadow root and frame, one block after another, so a change
//! inside them reads as a change of the page. Closed shadow roots stay out
//! of reach of any page script.

pub(crate) const SCAN: &str = r"const scan = () => {
  const roots = [], docs = [];
  let unread = 0;
  const shown = el => (!el.checkVisibility || el.checkVisibility()) || getComputedStyle(el).display === 'contents';
  const visit = root => {
    const all = root.querySelectorAll('*');
    for (let at = 0; at < all.length; at++) {
      const el = all[at];
      if (el.shadowRoot) { roots.push(el.shadowRoot); visit(el.shadowRoot); }
      if (el.localName !== 'iframe' && el.localName !== 'frame') continue;
      if (!shown(el)) continue;
      let inner = null;
      try { inner = el.contentDocument; } catch (error) { inner = null; }
      if (inner && inner.documentElement) { docs.push(inner); visit(inner); continue; }
      const box = el.getBoundingClientRect();
      if (box.width > 1 && box.height > 1) unread += 1;
    }
  };
  visit(document);
  const text = () => {
    const parts = [document.body ? document.body.innerText : ''];
    for (const root of roots) {
      for (const node of root.childNodes) {
        if (node.nodeType === 3) parts.push(node.textContent);
        else if (node.nodeType === 1 && node.localName !== 'slot' && shown(node)) parts.push(node.innerText || '');
      }
    }
    for (const doc of docs) parts.push(doc.body ? doc.body.innerText : '');
    return parts.join('\n');
  };
  return { roots, docs, unread, text };
};";

pub(crate) const START_WATCH: &str = r"(() => {
  SCAN
  const key = Symbol.for('tabref.watch');
  const old = window[key];
  if (old) old.observer.disconnect();
  const state = { count: 0 };
  state.observer = new MutationObserver(records => { state.count += records.length; });
  const found = scan();
  const options = { subtree: true, childList: true, attributes: true, characterData: true };
  for (const target of [document, ...found.roots, ...found.docs]) state.observer.observe(target, options);
  window[key] = state;
  const text = found.text();
  return { url: location.href, title: document.title, text: text.length > LIMIT ? null : text };
})()";

pub(crate) const READ_WATCH: &str = r"(() => {
  SCAN
  const key = Symbol.for('tabref.watch');
  const state = window[key];
  let mutations = null;
  if (state) {
    mutations = state.count + state.observer.takeRecords().length;
    state.observer.disconnect();
    delete window[key];
  }
  const found = scan();
  const text = found.text();
  return { url: location.href, title: document.title, mutations, unread: found.unread, text: text.length > LIMIT ? null : text };
})()";

pub(crate) const COUNT_WATCH: &str = r"(() => {
  const state = window[Symbol.for('tabref.watch')];
  if (!state) return null;
  state.count += state.observer.takeRecords().length;
  return state.count;
})()";
