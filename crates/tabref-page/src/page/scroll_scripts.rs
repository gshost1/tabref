//! The page scripts a scroll runs. Each gets the scroll's site as `this`;
//! `PRELUDE` is pasted in first. A scroll that would end within reach of
//! the end of the content is only planned by `SCROLL` and made by `WATCH`,
//! after tabref turned network events on, so the request a sentinel starts
//! with the next frame is seen.

pub(crate) const PRELUDE: &str = r"const anchor = this && this.nodeType === 1 ? this : null;
  const doc = anchor ? anchor.ownerDocument : document;
  const win = doc.defaultView;
  const root = doc.scrollingElement || doc.documentElement;
  const key = Symbol.for('tabref.scroll');
  const requests = Symbol.for('tabref.scroll-requests');
  const parent = node => node.parentElement || (node.getRootNode() || {}).host;
  const left = (el, dx, dy) => dy > 0 ? el.scrollHeight - el.clientHeight - el.scrollTop
    : dy < 0 ? el.scrollTop
    : dx > 0 ? el.scrollWidth - el.clientWidth - el.scrollLeft : el.scrollLeft;
  const skipped = new Set(['SCRIPT', 'STYLE', 'TEMPLATE', 'NOSCRIPT']);
  const shown = (node) => node.isConnected && !skipped.has(node.tagName)
    && node.getClientRects().length > 0 && (node.innerText || '').trim() !== '';
  const grown = (state) => (state.dy ? state.target.scrollHeight : state.target.scrollWidth) > state.extent + 1;
  const pool = (state) => grown(state) ? state.added.concat(state.swapped) : state.added;";

pub(crate) const SCROLL: &str = r"function(dx, dy, nearViews) {
  PRELUDE
  WATCH_REQUESTS
  const scrolls = (el) => {
    if (!el || el.nodeType !== 1) return false;
    const style = win.getComputedStyle(el);
    const overflow = style.overflowY + ' ' + style.overflowX;
    return /(auto|scroll|overlay)/.test(overflow)
      && (el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth);
  };
  const shift = (el) => {
    const before = [el.scrollLeft, el.scrollTop];
    if (el === root) win.scrollBy(dx, dy); else el.scrollBy(dx, dy);
    return el.scrollLeft !== before[0] || el.scrollTop !== before[1];
  };
  let target = anchor;
  if (target) {
    while (target && !scrolls(target)) target = parent(target);
    target = target || root;
  } else if (left(root, dx, dy) < 1) {
    target = doc.elementFromPoint(win.innerWidth / 2, win.innerHeight / 2);
    while (target && !scrolls(target)) target = parent(target);
    target = target || Array.from(doc.querySelectorAll('body *')).filter(scrolls)
      .sort((a, b) => b.clientHeight * b.clientWidth - a.clientHeight * a.clientWidth)[0] || root;
  } else {
    target = root;
  }
  if (win[key]) { if (win[key].observer) win[key].observer.disconnect(); delete win[key]; }
  const hidden = doc.visibilityState === 'hidden';
  const view = target === root ? (dy ? win.innerHeight : win.innerWidth)
    : (dy ? target.clientHeight : target.clientWidth);
  const before = left(target, dx, dy);
  if (before >= 1 && before - Math.abs(dy || dx) <= view * nearViews) {
    win[key] = { target, dx, dy, planned: true };
    return { moved: false, near: true, atEnd: false, hidden };
  }
  const moved = anchor || target !== root || before >= 1 ? shift(target) : false;
  return { moved, near: false, atEnd: left(target, dx, dy) < 2, hidden,
    pending: Boolean(win[requests]?.pending) };
}";

pub(crate) const WATCH_REQUESTS: &str = r"if (!win[requests]) {
  const tracker = { pending: 0 };
  const fetch = win.fetch;
  const send = win.XMLHttpRequest.prototype.send;
  win.fetch = function(...args) {
    const response = Reflect.apply(fetch, this, args);
    tracker.pending += 1;
    return response.then(
      value => { tracker.pending -= 1; return value; },
      error => { tracker.pending -= 1; throw error; }
    );
  };
  win.XMLHttpRequest.prototype.send = function(...args) {
    const done = () => { tracker.pending -= 1; };
    this.addEventListener('loadend', done, { once: true });
    tracker.pending += 1;
    try { return Reflect.apply(send, this, args); }
    catch (error) { this.removeEventListener('loadend', done); done(); throw error; }
  };
  win[requests] = tracker;
}";

pub(crate) const WATCH: &str = r"function() {
  PRELUDE
  const state = win[key];
  if (!state || !state.planned) return null;
  state.planned = false;
  const el = state.target, dx = state.dx, dy = state.dy;
  Object.assign(state, { added: [], swapped: [], mutations: 0, extent: dy ? el.scrollHeight : el.scrollWidth });
  const observe = target => state.observer.observe(target, { subtree: true, childList: true, characterData: true });
  const observeRoots = node => {
    if (node.shadowRoot) { observe(node.shadowRoot); observeRoots(node.shadowRoot); }
    for (const child of node.querySelectorAll('*')) {
      if (child.shadowRoot) { observe(child.shadowRoot); observeRoots(child.shadowRoot); }
    }
  };
  const inside = node => {
    for (let ancestor = node; ancestor; ancestor = ancestor.getRootNode().host) {
      if (el.contains(ancestor)) return true;
    }
    return false;
  };
  state.note = (records) => {
    for (const record of records) {
      if (!inside(record.target)) continue;
      state.mutations += 1;
      const into = record.removedNodes.length ? state.swapped : state.added;
      for (const node of record.addedNodes) {
        if (node.nodeType !== 1) continue;
        observeRoots(node);
        into.push(node);
      }
    }
  };
  state.observer = new win.MutationObserver(state.note);
  observe(doc);
  observeRoots(el);
  const before = [el.scrollLeft, el.scrollTop];
  if (el === root) win.scrollBy(dx, dy); else el.scrollBy(dx, dy);
  const moved = el.scrollLeft !== before[0] || el.scrollTop !== before[1];
  return { moved, atEnd: left(el, dx, dy) < 2 };
}";

pub(crate) const FRAMES: &str = r"function() {
  PRELUDE
  if (doc.visibilityState === 'hidden') return false;
  return new Promise(resolve => {
    let done = false;
    const finish = (framed) => { if (!done) { done = true; resolve(framed); } };
    const after = (then) => {
      const channel = new win.MessageChannel();
      channel.port1.onmessage = then;
      channel.port2.postMessage(0);
    };
    win.requestAnimationFrame(() => after(() => after(() => finish(true))));
    win.setTimeout(() => finish(false), 100);
  });
}";

pub(crate) const COUNT: &str = r"function() {
  PRELUDE
  const state = win[key];
  if (!state || !state.observer) return null;
  state.note(state.observer.takeRecords());
  return {
    mutations: state.mutations,
    inserted: state.added.length,
    items: pool(state).filter(shown).length,
    grew: grown(state),
  };
}";

pub(crate) const READ: &str = r"function(budget, lineChars) {
  PRELUDE
  const state = win[key];
  if (!state || !state.observer) return null;
  state.note(state.observer.takeRecords());
  state.observer.disconnect();
  delete win[key];
  const added = new Set(pool(state).filter(shown));
  const outermost = (node) => {
    for (let up = parent(node); up; up = parent(up)) if (added.has(up)) return false;
    return true;
  };
  let items = Array.from(added).filter(outermost);
  const repeats = (node) => {
    const kids = Array.from(node.children).filter(shown);
    return kids.length >= 3 && kids.filter(kid => kid.tagName === kids[0].tagName).length >= kids.length * 0.8;
  };
  while (items.length === 1 && repeats(items[0])) items = Array.from(items[0].children).filter(shown);
  const cut = (text, limit) => {
    const chars = Array.from(text);
    return chars.length <= limit ? text : chars.slice(0, limit - 1).join('') + '…';
  };
  const texts = items.map(node => node.innerText.trim()).filter(Boolean);
  const lines = [];
  let used = 0, more = 0;
  for (const text of texts) {
    for (const line of text.split('\n').map(part => part.trim()).filter(Boolean)) {
      const kept = cut(line, lineChars);
      if (more || (used + kept.length > budget && lines.length)) { more += 1; continue; }
      used += kept.length;
      lines.push(kept);
    }
  }
  const head = (text) => cut(text.split('\n')[0].trim(), lineChars);
  return {
    items: texts.length,
    first: texts.length ? head(texts[0]) : null,
    last: texts.length ? head(texts[texts.length - 1]) : null,
    lines, more,
    mutations: state.mutations,
    pending: Boolean(win[requests]?.pending),
    grew: grown(state),
    atEnd: left(state.target, state.dx, state.dy) < 2,
  };
}";
