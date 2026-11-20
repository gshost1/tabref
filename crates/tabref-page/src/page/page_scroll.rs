//! The page side of a scroll. A scroll far from the end of the content is
//! one script. One that would end within reach of the end is planned
//! first, then made by a second script that starts a `MutationObserver`
//! on the document, kept under a registered symbol of the window so page
//! scripts that list the window's keys never see it. Later scripts count
//! what it saw and read the elements the scroll's loading added.

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::scroll_scripts::{COUNT, FRAMES, PRELUDE, READ, SCROLL, WATCH, WATCH_REQUESTS};
use super::scroll_site::{ScrollSite, SiteScript};
use crate::{CdpConnection, PageSession};

const NEAR_VIEWS: f64 = 1.5;
const REQUEST_MAX_AGE_MS: u64 = 3_000;
const LINE_BUDGET: u64 = 600;
const LINE_CHARS: u64 = 100;

/// Where a scroll left its target: whether it moved, whether it would end
/// within one and a half views of the end of the content, so it was only
/// planned and [`watch_scroll`] makes it, whether it is at the very end,
/// and whether the page says it is hidden. A scroll that could not move
/// fires no scroll event and changes no intersection. It can still see
/// whether an earlier scroll's requests are pending.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScrollStep {
    pub(crate) moved: bool,
    pub(crate) near: bool,
    pub(crate) at_end: bool,
    pub(crate) hidden: bool,
    pub(crate) pending: bool,
}

/// What the watch saw so far: DOM mutations inside the scrolled target,
/// elements inserted without replacing anything, those of them that show
/// text, and whether the scrolled content grew.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScrollCount {
    pub(crate) mutations: u64,
    pub(crate) inserted: u64,
    pub(crate) items: u64,
    pub(crate) grew: bool,
}

/// What the scroll's loading added, read when it settled.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ScrollLoaded {
    pub(crate) items: u64,
    pub(crate) first: Option<String>,
    pub(crate) last: Option<String>,
    pub(crate) lines: Vec<String>,
    pub(crate) more_lines: u64,
    pub(crate) mutations: u64,
    pub(crate) grew: bool,
    pub(crate) at_end: bool,
    pub(crate) pending: bool,
}

fn run(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    (source, arguments): (&str, &[Value]),
    deadline: Deadline,
) -> Result<Value, AppError> {
    let function = source
        .replace("PRELUDE", PRELUDE)
        .replace("WATCH_REQUESTS", WATCH_REQUESTS);
    let script = SiteScript {
        function: &function,
        arguments,
    };
    site.call(connection, session, script, deadline)
}

fn flag(reply: &Value, key: &str) -> bool {
    reply.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn number(reply: &Value, key: &str) -> u64 {
    reply.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// Scrolls `site` by `(dx, dy)` CSS pixels: the given element's nearest
/// scrollable container, or the document when it scrolls, otherwise the
/// scrollable container under the viewport's centre, or the largest one,
/// as an app whose body never scrolls lays out. A scroll that would end
/// near the end of the content is only planned.
pub(crate) fn scroll_step(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    delta: (f64, f64),
    deadline: Deadline,
) -> Result<ScrollStep, AppError> {
    let arguments = [
        json!(delta.0),
        json!(delta.1),
        json!(NEAR_VIEWS),
        json!(REQUEST_MAX_AGE_MS),
    ];
    let reply = run(connection, session, site, (SCROLL, &arguments), deadline)?;
    Ok(ScrollStep {
        moved: flag(&reply, "moved"),
        near: flag(&reply, "near"),
        at_end: flag(&reply, "atEnd"),
        hidden: flag(&reply, "hidden"),
        pending: flag(&reply, "pending"),
    })
}

/// Starts the watch and makes the scroll [`scroll_step`] planned. `None`
/// when the plan is gone, as after a navigation.
pub(crate) fn watch_scroll(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<Option<(bool, bool, f64)>, AppError> {
    let reply = run(connection, session, site, (WATCH, &[]), deadline)?;
    Ok(reply["started"]
        .as_f64()
        .map(|started| (flag(&reply, "moved"), flag(&reply, "atEnd"), started)))
}

/// Waits for the next animation frame and two tasks after it, so that
/// scroll events (dispatched as the frame begins) and
/// `IntersectionObserver` callbacks (queued as a task while it renders)
/// the scroll set off have run. `false` when no frame came within 100 ms
/// or the page is hidden: then neither ran.
pub(crate) fn wait_frames(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<bool, AppError> {
    Ok(run(connection, session, site, (FRAMES, &[]), deadline)?.as_bool() == Some(true))
}

/// What the watch saw so far. `None` once the watch is gone.
pub(crate) fn count_added(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<Option<ScrollCount>, AppError> {
    let reply = run(connection, session, site, (COUNT, &[]), deadline)?;
    if reply.is_null() {
        return Ok(None);
    }
    Ok(Some(ScrollCount {
        mutations: number(&reply, "mutations"),
        inserted: number(&reply, "inserted"),
        items: number(&reply, "items"),
        grew: flag(&reply, "grew"),
    }))
}

/// Stops the watch and reads what it added: the outermost added elements
/// that show text are the items (a single added wrapper of three or more
/// alike children stands for them), their lines cut to 100 characters as
/// far as 600 characters go, the rest only counted, and whether the target
/// is still at its end. Elements that replaced others count only when the
/// content grew, so a ticking clock is no item.
pub(crate) fn read_loaded(
    connection: &mut CdpConnection,
    session: &PageSession,
    site: ScrollSite<'_>,
    deadline: Deadline,
) -> Result<Option<ScrollLoaded>, AppError> {
    let arguments = [json!(LINE_BUDGET), json!(LINE_CHARS)];
    let reply = run(connection, session, site, (READ, &arguments), deadline)?;
    if reply.is_null() {
        return Ok(None);
    }
    let text = |key: &str| reply.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(Some(ScrollLoaded {
        items: number(&reply, "items"),
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
        more_lines: number(&reply, "more"),
        mutations: number(&reply, "mutations"),
        grew: flag(&reply, "grew"),
        at_end: flag(&reply, "atEnd"),
        pending: flag(&reply, "pending"),
    }))
}
