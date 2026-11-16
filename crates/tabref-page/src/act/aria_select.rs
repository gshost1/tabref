//! `select` on the ARIA patterns a page builds instead of a `<select>`: a
//! combobox (select-only or editable), a button with `aria-haspopup=listbox`,
//! or a single-select listbox. It does what a user does: opens the popup when
//! it is closed (a click, then Down Arrow when the control has the focus),
//! waits for its options, types the wanted text into an editable combobox
//! when no shown option matches, matches the option as a native select's is
//! matched, clicks it, and reads the control back. Only shown options are
//! matched: one a filter hid cannot be clicked.
//!
//! A pick counts only on a sign the click itself gave: the control's value
//! became the label, or, when it already read so (the typed text), the
//! option became selected or the popup closed while the control kept the
//! focus. Any failure after the popup opened closes it, and text typed for
//! the search is replaced by the old text again.

use tabref_core::{AppError, Deadline, DeliverySemantics};

use super::aria_select_errors::{
    multi_select, no_options, not_taken, option_gone, put_back, quoted, unpicked, unsupported,
};
use super::aria_select_read::{Control, Listed};
use super::click::click_times;
use super::fill::fill;
use super::key_press::press_key;
use super::option_match::{OptionEntry, OptionPick, pick_option};
use super::select_outcome::SelectOutcome;
use crate::{CdpConnection, ElementToken, PageSession, cut_chars};

const VALUE_CHARS: usize = 80;

/// What the select changed on the page so far, for the clean-up.
#[derive(Default)]
struct Touched {
    opened: bool,
    typed: bool,
}

/// Picks the option `choice` names in the ARIA control `token` names.
pub(crate) fn select_listed(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    choice: &str,
    deadline: Deadline,
) -> Result<SelectOutcome, AppError> {
    let mut page = Control {
        connection,
        session,
        token,
        deadline,
    };
    let first = page.read(true)?;
    if first.kind == "none" {
        return Err(unsupported(token, &first.describe));
    }
    if first.multi {
        return Err(multi_select(token));
    }
    let previous = first.value.clone();
    let mut touched = Touched::default();
    match pick(&mut page, first, choice, &mut touched) {
        Ok((outcome, close)) => {
            if close && touched.opened {
                let _ = press_key(page.connection, page.session, "Escape", page.deadline);
            }
            Ok(outcome)
        }
        Err(error) => Err(clean_up(&mut page, error, &touched, &previous)),
    }
}

/// The pick, and whether the popup tabref opened is still to be closed.
fn pick(
    page: &mut Control<'_>,
    mut state: Listed,
    choice: &str,
    touched: &mut Touched,
) -> Result<(SelectOutcome, bool), AppError> {
    let previous = state.value.clone();
    if !state.open {
        let token = page.token.clone();
        page.ready(&token, &format!("{} {}", state.kind, state.describe))?;
        click_times(page.connection, page.session, &token, 1, page.deadline)?;
        touched.opened = true;
        state = page.wait_for(|now| now.open)?;
        if !state.open && !state.editable && state.kind != "listbox" && state.focused {
            press_key(page.connection, page.session, "ArrowDown", page.deadline)?;
            state = page.wait_for(|now| now.open)?;
        }
    }
    if state.multi {
        return Err(multi_select(page.token));
    }
    if state.editable && pick_shown(&state, choice).0 == OptionPick::Missing {
        let field = page.field()?;
        touched.typed = true;
        touched.opened = true;
        fill(page.connection, page.session, &field, choice, page.deadline)?;
        state =
            page.wait_for(|now| now.open && pick_shown(now, choice).0 != OptionPick::Missing)?;
    }
    let (mut index, label) = chosen(&state, choice, touched.typed)?;
    let already = if state.kind == "listbox" {
        state.options[index].selected
    } else {
        !touched.typed && previous.to_lowercase() == label.to_lowercase()
    };
    if already {
        return Ok((outcome(&previous, &previous, &label), true));
    }
    let option = match page.option(index, &label)? {
        Some(option) => option,
        None => {
            state = page.read(false)?;
            let (again, label_again) = chosen(&state, choice, touched.typed)?;
            if label_again != label {
                return Err(option_gone(&label));
            }
            index = again;
            page.option(index, &label)?
                .ok_or_else(|| option_gone(&label))?
        }
    };
    page.ready(&option, &format!("option {}", quoted(&label)))?;
    click_times(page.connection, page.session, &option, 1, page.deadline)?;
    let before = state;
    let after = page.wait_for(|now| took(now, &before, index, &label))?;
    if !took(&after, &before, index, &label) {
        return Err(not_taken(&label, &after.value));
    }
    Ok((outcome(&after.value, &previous, &label), after.open))
}

fn outcome(value: &str, previous: &str, label: &str) -> SelectOutcome {
    SelectOutcome {
        value: cut_chars(value, VALUE_CHARS),
        label: label.to_owned(),
        previous: (!previous.is_empty()).then(|| cut_chars(previous, VALUE_CHARS)),
        read_back: true,
    }
}

/// The index and label of the one shown option `choice` names, or the
/// refusal that lists the shown options.
fn chosen(state: &Listed, choice: &str, typed: bool) -> Result<(usize, String), AppError> {
    let (pick, indices) = pick_shown(state, choice);
    match pick {
        OptionPick::Chosen(at) => {
            let index = indices[at];
            Ok((index, state.options[index].entry.label.clone()))
        }
        other if state.open || state.options.iter().any(|option| option.shown) => {
            let (entries, _) = shown_entries(state);
            Err(unpicked(&entries, choice, &other))
        }
        _ => Err(no_options(&state.describe, choice, typed)),
    }
}

/// Puts back the text tabref typed and closes the popup it opened, and
/// says so in `error`.
fn clean_up(
    page: &mut Control<'_>,
    error: AppError,
    touched: &Touched,
    previous: &str,
) -> AppError {
    let mut error = error;
    if touched.typed {
        let restored = page
            .field()
            .and_then(|field| {
                fill(
                    page.connection,
                    page.session,
                    &field,
                    previous,
                    page.deadline,
                )
            })
            .is_ok_and(|filled| filled.verified);
        error.message.push_str(&put_back(previous, restored));
    }
    if touched.opened {
        let _ = press_key(page.connection, page.session, "Escape", page.deadline);
        error = error.with_disposition(DeliverySemantics::delivered_unverified());
    }
    error
}

/// Whether the option click took. The value must read as the label: the
/// same, or holding it when no other option's label holds it too; a listbox
/// must have the option selected. And the click must have left a sign the
/// state before it could not give: the value changed, or the option became
/// selected, or the popup closed while the control kept the focus.
pub(crate) fn took(now: &Listed, before: &Listed, index: usize, label: &str) -> bool {
    let selected = |state: &Listed| {
        state
            .options
            .get(index)
            .is_some_and(|option| option.selected)
    };
    if now.kind == "listbox" {
        return selected(now)
            && now
                .options
                .get(index)
                .is_some_and(|option| option.entry.label == label);
    }
    let value = now.value.to_lowercase();
    let wanted = label.to_lowercase();
    let shared = before.options.iter().chain(&now.options).any(|option| {
        let other = option.entry.label.to_lowercase();
        other != wanted && other.contains(&wanted)
    });
    let shows = value == wanted || (!shared && value.contains(&wanted));
    let changed = now.value != before.value;
    let committed =
        (selected(now) && !selected(before)) || (before.open && !now.open && now.focused);
    shows && (changed || committed)
}

/// The shown options as entries, and the index of each in all options.
pub(crate) fn shown_entries(state: &Listed) -> (Vec<OptionEntry>, Vec<usize>) {
    state
        .options
        .iter()
        .enumerate()
        .filter(|(_, option)| option.shown)
        .map(|(index, option)| (option.entry.clone(), index))
        .unzip()
}

pub(crate) fn pick_shown(state: &Listed, choice: &str) -> (OptionPick, Vec<usize>) {
    let (entries, indices) = shown_entries(state);
    (pick_option(&entries, choice), indices)
}

#[cfg(test)]
#[path = "aria_select_tests.rs"]
mod tests;
