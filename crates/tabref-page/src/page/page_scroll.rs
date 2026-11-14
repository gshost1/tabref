use tabref_core::{AppError, Deadline};

use super::evaluate::evaluate_quietly;
use crate::{CdpConnection, PageSession};

const SCROLL_PAGE: &str = r"((dx, dy) => {
  const moved = (el, run) => {
    const before = [el.scrollLeft, el.scrollTop];
    run();
    return el.scrollLeft !== before[0] || el.scrollTop !== before[1];
  };
  const root = document.scrollingElement || document.documentElement;
  if (moved(root, () => window.scrollBy(dx, dy))) return true;
  const scrolls = (el) => {
    const style = getComputedStyle(el);
    const overflow = style.overflowY + ' ' + style.overflowX;
    return /(auto|scroll|overlay)/.test(overflow)
      && (el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth);
  };
  let el = document.elementFromPoint(innerWidth / 2, innerHeight / 2);
  while (el && !scrolls(el)) el = el.parentElement;
  if (!el) {
    el = Array.from(document.querySelectorAll('body *')).filter(scrolls)
      .sort((a, b) => b.clientHeight * b.clientWidth - a.clientHeight * a.clientWidth)[0];
  }
  return el ? moved(el, () => el.scrollBy(dx, dy)) : false;
})";

/// Scrolls the page by `(dx, dy)` CSS pixels: the document when it scrolls,
/// otherwise the scrollable container under the viewport's centre, or the
/// largest one, as an app whose body never scrolls lays out. Returns whether
/// anything moved.
pub fn scroll_page(
    connection: &mut CdpConnection,
    session: &PageSession,
    delta: (f64, f64),
    deadline: Deadline,
) -> Result<bool, AppError> {
    let expression = format!("{SCROLL_PAGE}({}, {})", delta.0, delta.1);
    Ok(evaluate_quietly(connection, session, &expression, deadline)?.as_bool() == Some(true))
}
