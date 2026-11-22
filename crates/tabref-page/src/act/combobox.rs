use std::time::Duration;

use serde_json::Value;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::combobox_popup_report::{PopupText, WATCHED_TEXT};
use super::combobox_search::{PopupSearch, initially_open, is_popup};
use super::combobox_state::{ComboboxEvidence, ComboboxState, option_selected, read};
use crate::act::element_call::ElementCall;
use crate::act::element_errors::stale;
use crate::act::remote_element::{call_on_element, prove_document};
use crate::locate::name_match::normalize;
use crate::{
    ActionNeed, CdpConnection, DomTree, ElementToken, Locator, PageSession, SelectOutcome,
    capture_dom, click, fill, press_key, wait_actionable,
};

const CLOSE_WAIT: Duration = Duration::from_millis(250);
const POLL: Duration = Duration::from_millis(50);

const READ_OPTION: &str = r"function() {
  return [this.innerText ?? this.textContent, this.getAttribute('value')];
}";

const FOCUS_FOR_ESCAPE: &str = r"function() {
  const active = this.getRootNode().activeElement;
  if (!active || active === this.ownerDocument.body) this.focus({preventScroll: true});
}";

pub(super) fn choose(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    choice: &str,
    deadline: Deadline,
) -> Result<SelectOutcome, AppError> {
    let previous = read(connection, session, combo, deadline)?;
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
    let popup_text = if let Some(backend) = already_open
        && !search
            .options(connection, backend, deadline)?
            .children
            .is_empty()
    {
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
        ensure_clickable(connection, session, combo, "the combobox", deadline)?;
        if let Err(error) = click(connection, session, combo, deadline) {
            return Err(failed(connection, &search, &previous, error, deadline));
        }
        None
    };

    let mut queried = false;
    let picked = (|| {
        let (found, backend) = match search.find(connection, deadline, previous.editable) {
            Err(error)
                if previous.editable
                    && error.code == ErrorCode::ElementNotFound
                    && (previous.autocomplete
                        || error.details.as_ref().is_some_and(|details| {
                            details["kind"] == "combobox_list_incomplete"
                        })) =>
            {
                queried = true;
                fill(connection, session, combo, choice, deadline)?;
                search.find(connection, deadline, false)?
            }
            other => other?,
        };
        let label = found.name.unwrap_or_else(|| choice.to_owned());
        ensure_clickable(
            connection,
            session,
            &found.token,
            &format!("option {label:?}"),
            deadline,
        )?;
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
        let before_pick = read(connection, session, combo, deadline)?;
        click(connection, session, &found.token, deadline)
            .map_err(|error| error.with_disposition(DeliverySemantics::uncertain()))?;
        let selected = option_selected(connection, session, &found.token, deadline);
        Ok((
            backend,
            label,
            option_texts,
            before_pick,
            found.token,
            selected,
        ))
    })();
    let (backend, label, option_texts, before_pick, option_token, selected) = match picked {
        Ok(picked) => picked,
        Err(error) => {
            if queried
                && read(connection, session, combo, deadline)
                    .is_ok_and(|state| state.value != previous.value)
            {
                let _ = fill(connection, session, combo, &previous.value, deadline);
            }
            return Err(failed(connection, &search, &previous, error, deadline));
        }
    };
    let popup = Some(backend);
    let mut uncommitted = false;
    let popup_error = match popup_open(connection, &search, popup, deadline) {
        Ok(true) => {
            let after_pick = read(connection, session, combo, deadline);
            uncommitted = !selected
                && after_pick.as_ref().is_ok_and(|after| {
                    after.change_from(&before_pick).is_none()
                        && !after.matches(&before_pick, &option_texts)
                });
            match close(connection, &search, popup, deadline) {
                Ok(true) => None,
                Ok(false) => {
                    let kept = selected
                        || after_pick.is_ok_and(|after| after.matches(&before_pick, &option_texts));
                    let mut message = "the combobox popup stayed open after Escape".to_owned();
                    if !kept {
                        message.push_str(" (the select did not keep it)");
                    }
                    return Err(AppError::new(ErrorCode::ActionFailed, message)
                        .with_disposition(DeliverySemantics::delivered_unverified()));
                }
                Err(error) => Some(error.message),
            }
        }
        Ok(false) => None,
        Err(error) => Some(error.message),
    };
    Ok(SelectOutcome {
        label,
        previous: Some(previous.value.clone()),
        combobox: true,
        option_texts,
        popup_text,
        popup_error,
        previous_text: previous.text.clone(),
        readback: Some(ComboboxEvidence {
            before_pick,
            before_query: queried.then_some(previous),
            option_token,
            selected,
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

fn popup_open(
    connection: &mut CdpConnection,
    search: &PopupSearch<'_>,
    popup: Option<i64>,
    deadline: Deadline,
) -> Result<bool, AppError> {
    let dom = frame_dom(connection, search.session, search.combo, deadline)?;
    search.open(connection, &dom, popup, deadline)
}

fn close(
    connection: &mut CdpConnection,
    search: &PopupSearch<'_>,
    popup: Option<i64>,
    deadline: Deadline,
) -> Result<bool, AppError> {
    prove_document(connection, search.session, search.combo, deadline)?;
    let _ = call(
        connection,
        search.session,
        search.combo,
        FOCUS_FOR_ESCAPE,
        deadline,
    );
    press_key(connection, search.session, "Escape", deadline)?;
    let wait = deadline.capped(CLOSE_WAIT);
    loop {
        if !popup_open(connection, search, popup, wait)? {
            return Ok(true);
        }
        if wait.remaining() <= POLL {
            return Ok(false);
        }
        std::thread::sleep(POLL);
    }
}

fn failed(
    connection: &mut CdpConnection,
    search: &PopupSearch<'_>,
    previous: &ComboboxState,
    error: AppError,
    deadline: Deadline,
) -> AppError {
    let restored = close(connection, search, None, deadline) == Ok(true)
        && read(connection, search.session, search.combo, deadline)
            .is_ok_and(|state| &state == previous);
    let disposition = if restored && error.disposition != DeliverySemantics::uncertain() {
        DeliverySemantics::not_delivered()
    } else {
        DeliverySemantics::uncertain()
    };
    error.with_disposition(disposition)
}
