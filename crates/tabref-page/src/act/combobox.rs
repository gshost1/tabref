use std::time::Duration;

use serde_json::Value;
use tabref_cdp::is_timeout;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::combobox_popup_report::{PopupText, WATCHED_TEXT};
use super::combobox_search::{PopupSearch, SearchMode, incomplete_error, initially_open, is_popup};
use super::combobox_state::{
    ComboboxEvidence, ComboboxState, option_selected, read, restore_value,
};
use crate::act::element_call::ElementCall;
use crate::act::element_errors::stale;
use crate::act::remote_element::{call_on_element, prove_document};
use crate::locate::name_match::normalize;
use crate::{
    ActionNeed, CdpConnection, DomTree, ElementToken, Locator, PageSession, SelectOutcome,
    capture_dom, click, fill, hover, press_key, wait_actionable,
};

const CLOSE_WAIT: Duration = Duration::from_millis(250);
const POLL: Duration = Duration::from_millis(50);

const READ_OPTION: &str = r"function() {
  return [this.innerText ?? this.textContent, this.getAttribute('value'),
    ...Array.from(this.children, child => child.innerText ?? child.textContent)];
}";

const FOCUS_FOR_ESCAPE: &str = r"function() {
  const active = this.getRootNode().activeElement;
  if (!active || active === this.ownerDocument.body) this.focus({preventScroll: true});
}";

struct Attempt {
    previous: ComboboxState,
    typed: Option<String>,
}

pub(super) fn choose(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    choice: &str,
    deadline: Deadline,
) -> Result<SelectOutcome, AppError> {
    let mut attempt = Attempt {
        previous: read(connection, session, combo, deadline)?,
        typed: None,
    };
    let before = frame_dom(connection, session, combo, deadline)?;
    let already_open = initially_open(&before, combo).map(|node| node.backend_id);
    let search = PopupSearch {
        session,
        combo,
        baseline: before
            .nodes
            .iter()
            .filter(|node| is_popup(node) && Some(node.backend_id) != already_open)
            .map(|node| node.backend_id)
            .collect(),
        locator: Locator {
            role: Some("option".to_owned()),
            name: Some(choice.to_owned()),
            ..Locator::default()
        },
    };
    let initial = if let Some(backend) = already_open {
        let options = search.options(connection, backend, deadline)?;
        (!options.children.is_empty()).then_some((backend, options))
    } else {
        None
    };
    let reused = initial.as_ref().map(|(backend, _)| *backend);
    let popup_text = if let Some(backend) = reused {
        call(
            connection,
            session,
            &search.token(backend),
            WATCHED_TEXT,
            deadline,
        )
        .ok()
        .as_ref()
        .and_then(PopupText::from_reply)
    } else {
        ensure_clickable(connection, session, combo, "the combobox", deadline)
            .map_err(|error| error.with_disposition(DeliverySemantics::not_delivered()))?;
        if let Err(error) = click(connection, session, combo, deadline) {
            return Err(failed(connection, &search, &attempt, error, deadline));
        }
        None
    };

    let initial_search = if attempt.previous.editable {
        SearchMode::ReserveFilter
    } else {
        SearchMode::All
    };
    let picked = (|| {
        let (found, backend) = match search.find(connection, deadline, initial_search, initial) {
            Err(error)
                if attempt.previous.editable
                    && error.code == ErrorCode::ElementNotFound
                    && (attempt.previous.autocomplete
                        || error.details.as_ref().is_some_and(|details| {
                            details["kind"] == "combobox_list_incomplete"
                        })) =>
            {
                attempt.typed = Some(choice.to_owned());
                if let Some(observed) = fill(connection, session, combo, choice, deadline)?.observed
                {
                    attempt.typed = Some(observed);
                }
                let mode = if error
                    .details
                    .as_ref()
                    .is_some_and(|details| details["kind"] == "combobox_list_incomplete")
                {
                    SearchMode::ExactAfterIncomplete(error)
                } else {
                    SearchMode::All
                };
                search.find(connection, deadline, mode, None)?
            }
            other => other?,
        };
        let label = found.name.unwrap_or_else(|| choice.to_owned());
        let text = call(connection, session, &found.token, READ_OPTION, deadline)?;
        let option_texts = std::iter::once(label.as_str())
            .chain(
                text.as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str),
            )
            .map(normalize)
            .collect::<Vec<_>>();
        let was_selected = option_selected(connection, session, &found.token, deadline);
        let already_selected =
            was_selected == Some(true) || attempt.previous.has_matching_chip(&option_texts);
        if !already_selected {
            ensure_clickable(
                connection,
                session,
                &found.token,
                &format!("option {label:?}"),
                deadline,
            )?;
        }
        let before_press = if !already_selected && was_selected == Some(false) {
            hover(connection, session, &found.token, deadline)?;
            option_selected(connection, session, &found.token, deadline)
        } else {
            None
        };
        let before_pick = read(connection, session, combo, deadline)?;
        let selected = if already_selected {
            false
        } else {
            click(connection, session, &found.token, deadline).map_err(|error| {
                if error.disposition == DeliverySemantics::not_delivered() {
                    error
                } else {
                    error.with_disposition(DeliverySemantics::uncertain())
                }
            })?;
            before_press == Some(false)
                && option_selected(connection, session, &found.token, deadline) == Some(true)
        };
        Ok((
            backend,
            label,
            option_texts,
            before_pick,
            selected,
            already_selected,
        ))
    })();
    let (backend, label, option_texts, before_pick, selected, already_selected) = match picked {
        Ok(picked) => picked,
        Err(error) => {
            return Err(failed(connection, &search, &attempt, error, deadline));
        }
    };
    let closure = if already_selected && reused == Some(backend) {
        None
    } else {
        Some(close(connection, &search, Some(backend), deadline))
    };
    let uncommitted = !matches!(closure, Some(Ok(PopupClosure::AlreadyClosed)));
    let popup_error = match closure {
        Some(Ok(PopupClosure::StillOpen(_))) => {
            Some("the combobox popup stayed open after Escape".to_owned())
        }
        Some(Err(error)) => Some(error.message),
        _ => None,
    };
    let Attempt { previous, typed } = attempt;
    Ok(SelectOutcome {
        label,
        previous: Some(previous.value.clone()),
        combobox: true,
        option_texts,
        popup_text,
        popup_error,
        previous_text: previous.text.clone(),
        readback: Some(ComboboxEvidence {
            previous,
            before_pick,
            typed,
            selected,
            already_selected,
            uncommitted,
        }),
        ..SelectOutcome::default()
    })
}

pub(super) fn call(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    function: &'static str,
    deadline: Deadline,
) -> Result<Value, AppError> {
    call_on_element(
        connection,
        session,
        token,
        ElementCall {
            function,
            arguments: Vec::new(),
        },
        deadline,
    )
}

fn ensure_clickable(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    subject: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    if let Some(blocked) = wait_actionable(connection, session, token, ActionNeed::Click, deadline)?
    {
        return Err(blocked.error(subject));
    }
    Ok(())
}

pub(super) fn frame_dom(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    deadline: Deadline,
) -> Result<DomTree, AppError> {
    prove_document(connection, session, combo, deadline)?;
    capture_dom(connection, session, deadline)?
        .into_iter()
        .find(|tree| {
            tree.frame_id == combo.frame_id && tree.find_backend(combo.backend_node_id).is_some()
        })
        .ok_or_else(|| stale(combo, "the combobox is no longer in its document"))
}

fn visible_options(
    connection: &mut CdpConnection,
    search: &PopupSearch<'_>,
    popup: Option<i64>,
    deadline: Deadline,
) -> Result<usize, AppError> {
    let dom = frame_dom(connection, search.session, search.combo, deadline)?;
    search.visible_option_count(connection, &dom, popup, deadline)
}

#[derive(Debug, PartialEq, Eq)]
enum PopupClosure {
    AlreadyClosed,
    Dismissed(usize),
    StillOpen(usize),
}

fn close(
    connection: &mut CdpConnection,
    search: &PopupSearch<'_>,
    popup: Option<i64>,
    deadline: Deadline,
) -> Result<PopupClosure, AppError> {
    let seen = visible_options(connection, search, popup, deadline)?;
    if seen == 0 {
        return Ok(PopupClosure::AlreadyClosed);
    }
    let _ = call(
        connection,
        search.session,
        search.combo,
        FOCUS_FOR_ESCAPE,
        deadline,
    );
    if visible_options(connection, search, popup, deadline)? == 0 {
        return Ok(PopupClosure::Dismissed(seen));
    }
    press_key(connection, search.session, "Escape", deadline)?;
    let wait = deadline.capped(CLOSE_WAIT);
    loop {
        match visible_options(connection, search, popup, deadline) {
            Ok(0) => return Ok(PopupClosure::Dismissed(seen)),
            Ok(_) => {}
            Err(error) if is_timeout(&error) => return Ok(PopupClosure::StillOpen(seen)),
            Err(error) => return Err(error),
        }
        if wait.is_expired() {
            return Ok(PopupClosure::StillOpen(seen));
        }
        std::thread::sleep(POLL.min(wait.remaining()));
    }
}

fn failed(
    connection: &mut CdpConnection,
    search: &PopupSearch<'_>,
    attempt: &Attempt,
    mut error: AppError,
    deadline: Deadline,
) -> AppError {
    let closure = close(connection, search, None, deadline);
    if let Ok(PopupClosure::Dismissed(seen) | PopupClosure::StillOpen(seen)) = closure
        && error.code == ErrorCode::ElementNotFound
        && error
            .details
            .as_ref()
            .is_some_and(|details| details["options_total"] == 0)
    {
        error = incomplete_error(seen).with_disposition(error.disposition);
    }
    let closed = matches!(
        closure,
        Ok(PopupClosure::AlreadyClosed | PopupClosure::Dismissed(_))
    );
    let still_closed = attempt.typed.is_none()
        || match restore_value(
            connection,
            search.session,
            search.combo,
            &attempt.previous.value,
            deadline,
        ) {
            Ok(false) => true,
            Ok(true) => visible_options(connection, search, None, deadline) == Ok(0),
            Err(_) => false,
        };
    let restored = closed
        && still_closed
        && read(connection, search.session, search.combo, deadline)
            .is_ok_and(|state| state == attempt.previous);
    let disposition = if restored && error.disposition != DeliverySemantics::uncertain() {
        DeliverySemantics::not_delivered()
    } else {
        DeliverySemantics::uncertain()
    };
    error.with_disposition(disposition)
}

#[cfg(test)]
#[path = "combobox_tests.rs"]
mod tests;
