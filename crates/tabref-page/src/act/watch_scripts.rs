//! The page scripts behind the change watch. `SCAN` finds the open shadow
//! roots and same-origin frame documents of the page, nested either way,
//! and counts the shown frames from other sites, whose documents a page
//! script cannot read, when they are at least partly on screen and larger
//! than a tracking pixel. `text` is the rendered text of the document, then
//! of each shown shadow root and frame, one block after another, so a
//! change inside them reads as a change of the page. The document's text
//! has its own limit; the shadow and frame blocks share a second one.
//! Each block keeps its identity across both scans, so an unreadable or
//! oversized block can be excluded from both sides of the diff.

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
      if (inner && inner.documentElement) { docs.push([el, inner]); visit(inner); continue; }
      const box = el.getBoundingClientRect();
      const view = el.ownerDocument.defaultView;
      const onScreen = box.bottom > 0 && box.right > 0 && box.top < view.innerHeight && box.left < view.innerWidth;
      if (onScreen && box.width >= 10 && box.height >= 10) {
        unread += 1;
        docs.push([el, null]);
      }
    }
  };
  visit(document);
  const text = state => {
    const main = document.body ? document.body.innerText : '';
    const blocks = [[0, main.length <= LIMIT ? main : null]];
    let room = LIMIT;
    const add = (owner, part) => {
      if (!state.ids.has(owner)) state.ids.set(owner, state.nextId++);
      if (typeof part === 'string' && part.length > room) part = null;
      if (typeof part === 'string') room -= part.length;
      blocks.push([state.ids.get(owner), part]);
    };
    for (const root of roots) {
      const lines = [];
      let left = room;
      for (const node of root.childNodes) {
        const line = node.nodeType === 3 ? node.textContent :
          node.nodeType === 1 && node.localName !== 'slot' && shown(node) ? node.innerText : '';
        if (!line) continue;
        left -= line.length + (lines.length ? 1 : 0);
        if (left < 0) break;
        lines.push(line);
      }
      add(root, left < 0 ? null : lines.join('\n'));
    }
    for (const [frame, doc] of docs) add(frame, doc ? (doc.body ? doc.body.innerText : '') : false);
    return blocks;
  };
  return { roots, docs, unread, text };
};";

pub(crate) const START_WATCH: &str = r"(() => {
  SCAN
  const key = Symbol.for('tabref.watch');
  const old = window[key];
  if (old) old.observer.disconnect();
  const state = { count: 0, ids: new WeakMap(), nextId: 1 };
  state.observer = new MutationObserver(records => { state.count += records.length; });
  const found = scan();
  const options = { subtree: true, childList: true, attributes: true, characterData: true };
  for (const target of [document, ...found.roots]) state.observer.observe(target, options);
  for (const [, doc] of found.docs) if (doc) state.observer.observe(doc, options);
  window[key] = state;
  return { url: location.href, title: document.title, blocks: found.text(state) };
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
  const blocks = found.text(state || { ids: new WeakMap(), nextId: 1 });
  return { url: location.href, title: document.title, mutations, unread: found.unread, blocks };
})()";

pub(crate) const COUNT_WATCH: &str = r"(() => {
  const state = window[Symbol.for('tabref.watch')];
  if (!state) return null;
  state.count += state.observer.takeRecords().length;
  return state.count;
})()";
