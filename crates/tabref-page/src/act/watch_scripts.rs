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
  const popups = state => {
    const result = [];
    const id = node => {
      if (!state.ids.has(node)) state.ids.set(node, state.nextId++);
      return state.ids.get(node);
    };
    for (const [root, owner] of [[document, null], ...roots.map(root => [root, root]), ...docs.filter(([, doc]) => doc).map(([frame, doc]) => [doc, frame])]) {
      for (const popup of root.querySelectorAll('[role=listbox],[role=menu],[popover],[role=dialog],[role=alertdialog],dialog[open]')) {
        if (!shown(popup) || popup.closest('select')) continue;
        const dialog = popup.localName === 'dialog' || ['dialog', 'alertdialog'].includes(popup.getAttribute('role'));
        if (!dialog && popup.parentElement?.closest('[role=listbox],[role=menu],[popover]')) continue;
        const role = dialog ? (popup.getAttribute('role') === 'alertdialog' ? 'alertdialog' : 'dialog') :
          popup.getAttribute('role') === 'listbox' || popup.querySelector('[role=option]') ? 'listbox' : 'menu';
        const items = Array.from(popup.querySelectorAll('[role=option],[role=menuitem],[role=menuitemcheckbox],[role=menuitemradio]')).filter(shown);
        if (!dialog && !items.length) continue;
        const separate = items.every(item => {
          for (let node = item; node; node = node.parentElement) {
            if (!['block', 'list-item', 'flow-root'].includes(getComputedStyle(node).display)) return false;
            if (node === popup) return true;
          }
          return false;
        });
        const labels = (popup.getAttribute('aria-labelledby') || '').split(/\s+/).filter(Boolean).map(key => root.getElementById(key)?.textContent || '').join(' ').trim();
        const heading = dialog ? Array.from(popup.querySelectorAll('h1,h2,h3,h4,h5,h6,[role=heading]')).find(shown)?.innerText : '';
        const position = [];
        for (let node = popup; node.parentNode; node = node.parentNode) {
          position.push(Array.prototype.indexOf.call(node.parentNode.children || [], node));
        }
        result.push({ id: id(popup), watch: state.epoch, position: position.reverse().join('.'), block: owner ? id(owner) : 0, role,
          name: labels || popup.getAttribute('aria-label') || heading || '', count: items.length,
          text: dialog ? popup.innerText : separate ? items.map(item => item.innerText).join('\n') : null });
      }
    }
    return result;
  };
  return { roots, docs, unread, text, popups };
};";

pub(crate) const START_WATCH: &str = r"(() => {
  SCAN
  const key = Symbol.for('tabref.watch');
  const old = window[key];
  if (old) old.observer.disconnect();
  const state = { count: 0, ids: new WeakMap(), nextId: 1, epoch: Math.random().toString(36) };
  state.observer = new MutationObserver(records => { state.count += records.length; });
  const found = scan();
  const options = { subtree: true, childList: true, attributes: true, characterData: true };
  for (const target of [document, ...found.roots]) state.observer.observe(target, options);
  for (const [, doc] of found.docs) if (doc) state.observer.observe(doc, options);
  window[key] = state;
  return { url: location.href, title: document.title, blocks: found.text(state), popups: found.popups(state) };
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
  const current = state || { ids: new WeakMap(), nextId: 1, epoch: null };
  const blocks = found.text(current);
  return { url: location.href, title: document.title, mutations, unread: found.unread, blocks, popups: found.popups(current) };
})()";

pub(crate) const COUNT_WATCH: &str = r"(() => {
  const state = window[Symbol.for('tabref.watch')];
  if (!state) return null;
  state.count += state.observer.takeRecords().length;
  return state.count;
})()";
