//! The page scripts behind the change watch. `SCAN` finds the open shadow
//! roots and same-origin frame documents of the page, nested either way,
//! and counts the shown frames from other sites, whose documents a page
//! script cannot read, when they are at least partly on screen and larger
//! than a tracking pixel. `text` is the rendered text of the document, then
//! of each shown shadow root and frame, one block after another, so a
//! change inside them reads as a change of the page. The document's text
//! has its own limit; the shadow and frame blocks share a second one, and a
//! block that does not fit is left out, so a large frame never costs the
//! document's diff. Closed shadow roots stay out of reach of any page
//! script.

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
      const view = el.ownerDocument.defaultView;
      const onScreen = box.bottom > 0 && box.right > 0 && box.top < view.innerHeight && box.left < view.innerWidth;
      if (onScreen && box.width >= 10 && box.height >= 10) unread += 1;
    }
  };
  visit(document);
  const text = () => {
    const main = document.body ? document.body.innerText : '';
    if (main.length > LIMIT) return null;
    const parts = [main];
    let room = LIMIT;
    const add = part => { if (part && part.length <= room) { parts.push(part); room -= part.length; } };
    for (const root of roots) {
      for (const node of root.childNodes) {
        if (node.nodeType === 3) add(node.textContent);
        else if (node.nodeType === 1 && node.localName !== 'slot' && shown(node)) add(node.innerText || '');
      }
    }
    for (const doc of docs) add(doc.body ? doc.body.innerText : '');
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
  return { url: location.href, title: document.title, text };
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
  return { url: location.href, title: document.title, mutations, unread: found.unread, text };
})()";

pub(crate) const COUNT_WATCH: &str = r"(() => {
  const state = window[Symbol.for('tabref.watch')];
  if (!state) return null;
  state.count += state.observer.takeRecords().length;
  return state.count;
})()";
