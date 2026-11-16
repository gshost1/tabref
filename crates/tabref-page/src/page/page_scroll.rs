//! The page side of a scroll. One script moves the page or a container and,
//! when that leaves the end of the scrolled content within reach, starts a
//! `MutationObserver` on the document, kept under a registered symbol of
//! the window so page scripts that list the window's keys never see it.
//! Later scripts count what it saw and, at the end, read the elements the
//! scroll's loading added.

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::scroll_site::{ScrollSite, SiteScript};
use crate::{CdpConnection, PageSession};

const NEAR_VIEWS: f64 = 1.5;
const LINE_BUDGET: u64 = 600;

const PRELUDE: &str = r"const anchor = this && this.nodeType === 1 ? this : null;
  const doc = anchor ? anchor.ownerDocument : document;
  const win = doc.defaultView;
  const root = doc.scrollingElement || doc.documentElement;
  const key = Symbol.for('tabref.scroll');
  const left = (el, dx, dy) => dy > 0 ? el.scrollHeight - el.clientHeight - el.scrollTop
    : dy < 0 ? el.scrollTop
    : dx > 0 ? el.scrollWidth - el.clientWidth - el.scrollLeft : el.scrollLeft;";

const SCROLL: &str = r"function(dx, dy, nearViews) {
  PRELUDE
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
  let target = anchor, moved = false;
  if (target) {
    while (target && !scrolls(target)) target = target.parentElement;
    target = target || root;
    moved = shift(target);
  } else if (shift(root)) {
    target = root;
    moved = true;
  } else {
    target = doc.elementFromPoint(win.innerWidth / 2, win.innerHeight / 2);
    while (target && !scrolls(target)) target = target.parentElement;
    target = target || Array.from(doc.querySelectorAll('body *')).filter(scrolls)
      .sort((a, b) => b.clientHeight * b.clientWidth - a.clientHeight * a.clientWidth)[0] || root;
    if (target !== root) moved = shift(target);
  }
  const view = target === root ? (dy ? win.innerHeight : win.innerWidth)
    : (dy ? target.clientHeight : target.clientWidth);
  const remaining = left(target, dx, dy);
  const near = moved && remaining <= view * nearViews;
  if (win[key]) { win[key].observer.disconnect(); delete win[key]; }
  if (near) {
    const state = { added: [], mutations: 0, target, dx, dy };
    const note = (records) => {
      for (const record of records) {
        state.mutations += 1;
        for (const node of record.addedNodes) if (node.nodeType === 1) state.added.push(node);
      }
    };
    state.note = note;
    state.observer = new win.MutationObserver(note);
    state.observer.observe(doc, { subtree: true, childList: true, characterData: true });
    win[key] = state;
  }
  return { moved, near, atEnd: remaining < 2 };
}";

const FRAMES: &str = r"function() {
  PRELUDE
  return new Promise(resolve => {
    let done = false;
    const finish = () => { if (!done) { done = true; resolve(true); } };
    const after = (then) => {
      const channel = new win.MessageChannel();
      channel.port1.onmessage = then;
      channel.port2.postMessage(0);
    };
    win.requestAnimationFrame(() => after(() => after(finish)));
    win.setTimeout(finish, 100);
  });
}";

const COUNT: &str = r"function() {
  PRELUDE
  const state = win[key];
  if (!state) return null;
  state.note(state.observer.takeRecords());
  const items = state.added.filter(node => node.isConnected && (node.innerText || '').trim()).length;
  return { mutations: state.mutations, items };
}";

const READ: &str = r"function(budget) {
  PRELUDE
  const state = win[key];
  if (!state) return null;
  state.note(state.observer.takeRecords());
  state.observer.disconnect();
  delete win[key];
  const added = new Set(state.added.filter(node => node.isConnected));
  const outermost = (node) => {
    for (let up = node.parentElement; up; up = up.parentElement) if (added.has(up)) return false;
    return true;
  };
  let items = Array.from(added).filter(outermost);
  const repeats = (node) => {
    const kids = Array.from(node.children);
    return kids.length >= 3 && kids.filter(kid => kid.tagName === kids[0].tagName).length >= kids.length * 0.8;
  };
  while (items.length === 1 && repeats(items[0])) items = Array.from(items[0].children);
  const texts = items.map(node => (node.innerText || node.textContent || '').trim()).filter(Boolean);
  const lines = [];
  let used = 0, more = 0;
  for (const text of texts) {
    for (const line of text.split('\n').map(part => part.trim()).filter(Boolean)) {
      if (used + line.length > budget && lines.length) { more += 1; continue; }
      used += line.length;
      lines.push(line);
    }
  }
  const head = (text) => text.split('\n')[0].trim();
  return {
    items: texts.length,
    first: texts.length ? head(texts[0]) : null,
    last: texts.length ? head(texts[texts.length - 1]) : null,
    lines, more,
    atEnd: left(state.target, state.dx, state.dy) < 2,
  };
}";

/// Where a scroll left its target: whether it moved, whether it moved to
/// within one and a half views of the end of the content, so what it set
/// loading is being watched, and whether it is at the very end. A scroll
/// that could not move fires no scroll event and changes no intersection,
/// so it sets nothing loading and is not watched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScrollStep {
    pub(crate) moved: bool,
    pub(crate) near: bool,
    pub(crate) at_end: bool,
}

/// What the scroll's loading added, read when it settled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScrollLoaded {
    pub(crate) items: u64,
    pub(crate) first: Option<String>,
    pub(crate) last: Option<String>,
    pub(crate) lines: Vec<String>,
    pub(crate) more_lines: u64,
    pub(crate) at_end: bool,
}

fn script(source: &str) -> String {
    source.replace("PRELUDE", PRELUDE)
}

fn run(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    (source, arguments): (&str, &[Value]),
    deadline: Deadline,
) -> Result<Value, AppError> {
    let function = script(source);
    let script = SiteScript {
        function: &function,
        arguments,
    };
    site.call(connection, session, script, deadline)
}

/// Scrolls `site` by `(dx, dy)` CSS pixels: the given element's nearest
/// scrollable container, or the document when it scrolls, otherwise the
/// scrollable container under the viewport's centre, or the largest one,
/// as an app whose body never scrolls lays out.
pub(crate) fn scroll_step(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    delta: (f64, f64),
    deadline: Deadline,
) -> Result<ScrollStep, AppError> {
    let arguments = [json!(delta.0), json!(delta.1), json!(NEAR_VIEWS)];
    let reply = run(connection, session, site, (SCROLL, &arguments), deadline)?;
    let flag = |key: &str| reply.get(key).and_then(Value::as_bool).unwrap_or(false);
    Ok(ScrollStep {
        moved: flag("moved"),
        near: flag("near"),
        at_end: flag("atEnd"),
    })
}

/// Waits for the next animation frame and two tasks after it, or 100 ms
/// where no frame is drawn, so that scroll events (dispatched as the frame
/// begins) and `IntersectionObserver` callbacks (queued as a task while it
/// renders, before the second task) the scroll set off have run.
pub(crate) fn wait_frames(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<(), AppError> {
    run(connection, session, site, (FRAMES, &[]), deadline).map(|_| ())
}

/// The DOM mutations seen since the scroll, and how many added elements
/// with text are still in the document. `None` once the watch is gone.
pub(crate) fn count_added(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<Option<(u64, u64)>, AppError> {
    let reply = run(connection, session, site, (COUNT, &[]), deadline)?;
    let number = |key: &str| reply.get(key).and_then(Value::as_u64);
    Ok(number("mutations").zip(number("items")))
}

/// Stops the watch and reads what it added: the outermost added elements
/// with text are the items (a single added wrapper of three or more alike
/// children stands for them), their lines as far as 600 characters go, and whether the
/// target is still at its end.
pub(crate) fn read_loaded(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<ScrollLoaded, AppError> {
    let reply = run(
        connection,
        session,
        site,
        (READ, &[json!(LINE_BUDGET)]),
        deadline,
    )?;
    let text = |key: &str| reply.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(ScrollLoaded {
        items: reply.get("items").and_then(Value::as_u64).unwrap_or(0),
        first: text("first"),
        last: text("last"),
        lines: reply
            .get("lines")
            .and_then(Value::as_array)
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        more_lines: reply.get("more").and_then(Value::as_u64).unwrap_or(0),
        at_end: reply.get("atEnd").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Whether the page listens for scroll events where this scroll fires
/// them: on its window, its document, or the scrolled container. Such a
/// listener may load more after a debounce, so the settle gives it time.
pub(crate) fn listens_for_scroll(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<bool, AppError> {
    const PICK: &str = r"function(which) {
  PRELUDE
  const state = win[key];
  return which === 0 ? win : which === 1 ? doc : (state ? state.target : null);
}";
    let mut found = false;
    for which in 0..3 {
        let function = script(PICK);
        let arguments = [json!(which)];
        let pick = SiteScript {
            function: &function,
            arguments: &arguments,
        };
        let picked = site.object(connection, session, pick, deadline);
        let Ok(Some(object)) = picked else { continue };
        let reply = connection.call(
            "DOMDebugger.getEventListeners",
            json!({ "objectId": object, "depth": 0 }),
            Some(&session.session_id),
            deadline,
        );
        let Ok(reply) = reply else { continue };
        let listeners = reply.get("listeners").and_then(Value::as_array);
        if listeners.is_some_and(|listeners| {
            listeners.iter().any(|listener| {
                matches!(
                    listener.get("type").and_then(Value::as_str),
                    Some("scroll" | "scrollend")
                )
            })
        }) {
            found = true;
            break;
        }
    }
    ScrollSite::release(connection, session, deadline);
    Ok(found)
}
