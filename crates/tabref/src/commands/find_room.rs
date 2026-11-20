//! What a `find` with no match adds when the page could show more by
//! scrolling. The area last scrolled by tabref must still have room and
//! have grown since the scroll or fetched data after it. A box is named
//! by a control whose name is unique across the page.

use serde_json::Value;
use tabref_page::evaluate;

use crate::session::Session;
use crate::view::node_line::quote_text;

const ITEM_CHARS: usize = 40;

const ROOM_SCRIPT: &str = r#"(() => {
  const last = window[Symbol.for('tabref.last-vertical-scroll')];
  if (!last?.target.isConnected) return { loads: false };
  const root = document.scrollingElement || document.documentElement;
  const target = last.target;
  const room = target.scrollHeight - target.clientHeight - target.scrollTop;
  if (room <= 2) return { loads: false };
  const dataRequest = entry => {
    if (entry.startTime < last.started ||
        (entry.initiatorType !== 'fetch' && entry.initiatorType !== 'xmlhttprequest')) return false;
    try {
      const url = new URL(entry.name, location.href);
      return !/(?:^|[.\/-])(?:analytics|beacons?|collect|metrics|telemetry|tracking|doubleclick|posthog)(?:$|[.\/_-])/i
        .test(url.hostname + url.pathname);
    } catch { return false; }
  };
  const loads = performance.getEntriesByType('resource')
    .some(dataRequest) || target.scrollHeight > last.height + 1;
  if (!loads) return { loads: false };
  const facts = { loads: true, page_left: target === root ? room : 0, view: root.clientHeight };
  if (target === root) return facts;
  const nameOf = (node) => {
    const name = ((node.getAttribute('aria-label') || '').trim() || (node.innerText || '').trim());
    return name.includes('\n') ? '' : name;
  };
  const selector = 'a[href],button,input,select,textarea,summary,[role],[tabindex]';
  const names = [...target.querySelectorAll(selector)].slice(-50).map(nameOf).reverse();
  const counts = new Map(names.filter(Boolean).map(name => [name.toLowerCase(), 0]));
  facts.list = '';
  let seen = 0;
  const scan = tree => {
    for (const node of tree.querySelectorAll('*')) {
      if (++seen > 5000) return false;
      if (node.matches(selector)) {
        const name = nameOf(node).toLowerCase();
        if (counts.has(name)) counts.set(name, counts.get(name) + 1);
      }
      if (node.shadowRoot && !scan(node.shadowRoot)) return false;
      if (node.matches('iframe,frame') && (!node.contentDocument || !scan(node.contentDocument))) return false;
    }
    return true;
  };
  if (counts.size && scan(document)) {
    facts.list = names.find(name => counts.get(name.toLowerCase()) === 1) || '';
  }
  return facts;
})()"#;

/// Where the page could scroll to load more: the window, a scroll box
/// (named by a control in it when one is safe to name), or both when the
/// window still has more than a screen to go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Room {
    Page,
    List(Option<String>),
    Both(Option<String>),
}

/// Whether the page loads data and the page or a list on it scrolls
/// further. A script that fails or a page it cannot read claims nothing.
pub(crate) fn scroll_room(session: &mut Session) -> Option<Room> {
    let page = session.page().ok()?;
    let deadline = session.deadline();
    let value = evaluate(session.connection(), &page, ROOM_SCRIPT, deadline).ok()?;
    room_from(&value)
}

fn room_from(value: &Value) -> Option<Room> {
    if value.get("loads").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let left = value
        .get("page_left")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let view = value.get("view").and_then(Value::as_f64).unwrap_or(0.0);
    let page = left > 2.0;
    let list = value.get("list").and_then(Value::as_str).map(|item| {
        let item = item.trim();
        (!item.is_empty() && item.chars().count() <= ITEM_CHARS).then(|| item.to_owned())
    });
    match (page, list) {
        (false, None) => None,
        (true, None) => Some(Room::Page),
        (true, Some(item)) if left > view => Some(Room::Both(item)),
        (_, Some(item)) => Some(Room::List(item)),
    }
}

/// The note after `0 matches`: what scrolls further and the steps that
/// load more there.
pub(crate) fn room_note(room: &Room) -> String {
    let list_step = |item: &Option<String>| match item {
        Some(item) => format!("`scroll down {}`", quote_text(item)),
        None => "`scroll down \"<text in that list>\"`".to_owned(),
    };
    let (what, step) = match room {
        Room::Page => ("the page", "`scroll down`".to_owned()),
        Room::List(item) => ("a list on the page", list_step(item)),
        Room::Both(item) => (
            "the page and a list on it",
            format!("`scroll down` (page) or {} (list)", list_step(item)),
        ),
    };
    format!(
        "{what} scrolls further: if it loads more as it scrolls, {step} loads it; \
         repeat it in one tabref do, then find again"
    )
}

#[cfg(test)]
#[path = "find_room_tests.rs"]
mod tests;
