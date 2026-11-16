//! What a `find` with no match adds when the page could show more by
//! scrolling: the document, or a scroll container on it, is taller than
//! the space it shows and not scrolled to its end. tabref cannot know
//! whether scrolling loads anything, so the note says what to run if it
//! does, and nothing at all when nothing scrolls further. A list's note
//! names its last link or control, which `scroll down T` can target.

use serde_json::Value;
use tabref_page::evaluate;

use crate::session::Session;
use crate::view::node_line::quote_text;

const ITEM_CHARS: usize = 40;

const ROOM_SCRIPT: &str = r#"(() => {
  const root = document.scrollingElement || document.documentElement;
  const style = (node) => node ? getComputedStyle(node).overflowY : '';
  const html = style(document.documentElement), body = style(document.body);
  const locked = html === 'hidden' || html === 'clip' || (html === 'visible' && (body === 'hidden' || body === 'clip'));
  if (!locked && root.scrollHeight - root.clientHeight - root.scrollTop > 2) return { page: true };
  let best = null, area = 0, seen = 0;
  for (const node of document.body ? document.body.getElementsByTagName('*') : []) {
    if (++seen > 5000) break;
    if (node.clientHeight < 40 || node.scrollHeight - node.clientHeight - node.scrollTop <= 2) continue;
    const overflow = getComputedStyle(node).overflowY;
    if (overflow !== 'auto' && overflow !== 'scroll') continue;
    const size = node.clientHeight * node.clientWidth;
    if (size > area) { best = node; area = size; }
  }
  if (!best) return null;
  const controls = best.querySelectorAll('a[href],button,input,select,textarea,summary,[role],[tabindex]');
  for (let index = controls.length - 1; index >= 0 && index >= controls.length - 50; index--) {
    const text = (controls[index].innerText || controls[index].getAttribute('aria-label') || '').trim();
    if (text && !text.includes('\n')) return { list: text };
  }
  return { list: '' };
})()"#;

/// Where the page could scroll to show more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Room {
    Page,
    List(Option<String>),
}

/// Whether the page or a list on it scrolls further. A script that fails
/// or a page it cannot read claims nothing.
pub(crate) fn scroll_room(session: &mut Session) -> Option<Room> {
    let page = session.page().ok()?;
    let deadline = session.deadline();
    let value = evaluate(session.connection(), &page, ROOM_SCRIPT, deadline).ok()?;
    room_from(&value)
}

fn room_from(value: &Value) -> Option<Room> {
    if value.get("page").and_then(Value::as_bool) == Some(true) {
        return Some(Room::Page);
    }
    let item = value.get("list")?.as_str()?.trim();
    let item = (!item.is_empty() && item.chars().count() <= ITEM_CHARS).then(|| item.to_owned());
    Some(Room::List(item))
}

/// The note after `0 matches`: what scrolls further and the steps that
/// would load more there.
pub(crate) fn room_note(room: &Room) -> String {
    let (what, step) = match room {
        Room::Page => ("the page".to_owned(), "scroll down".to_owned()),
        Room::List(Some(item)) => (
            "a list on the page".to_owned(),
            format!("scroll down {}", quote_text(item)),
        ),
        Room::List(None) => (
            "a list on the page".to_owned(),
            "scroll down T (T: anything in that list)".to_owned(),
        ),
    };
    format!(
        "{what} scrolls further: if it loads more as it scrolls, `{step}` loads it; \
         repeat it in one tabref do, then find again"
    )
}

#[cfg(test)]
#[path = "find_room_tests.rs"]
mod tests;
