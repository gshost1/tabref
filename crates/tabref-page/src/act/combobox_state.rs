use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::combobox::call;
use crate::act::element_call::ElementCall;
use crate::act::element_errors::stale;
use crate::act::remote_element::call_on_element;
use crate::locate::name_match::normalize;
use crate::{CdpConnection, ElementToken, PageSession};

const READ_COMBOBOX: &str = r"function() {
  if (!this.isConnected) return null;
  const field = this.matches('input,textarea') ? this :
    this.querySelector('input:not([type=hidden]),textarea');
  const text = (this.innerText ?? this.textContent ?? '').trim();
  const autocomplete = this.getAttribute('aria-autocomplete');
  const editable = (field === this && !field.readOnly && !field.disabled) || this.isContentEditable;
  const tags = '[role=option][aria-selected=true], [data-chip], [data-tag], .chip, .tag, button, [role=button]';
  const scope = this.parentElement;
  const nearby = scope?.parentElement;
  const group = nearby && !nearby.matches('form, body, html') ? nearby : scope;
  const chips = Array.from(group?.querySelectorAll(tags) ?? []).filter(node => {
    if (node === this || this.contains(node) || !node.getClientRects().length || node.closest('[role=listbox]')) return false;
    if (node.matches('button, [role=button]')) {
      return /^remove\b/i.test(node.getAttribute('aria-label') ?? '') || node.matches('[data-chip], [data-tag], .chip, .tag');
    }
    return true;
  }).map(node => [node.innerText, node.getAttribute('aria-label')?.replace(/^\s*remove\b\s*/i, '')]
    .map(label => label?.trim()).filter(Boolean)).filter(labels => labels.length);
  const hiddenInputs = scope && !scope.matches('form, body, html')
    ? Array.from(scope.querySelectorAll('input[type=hidden]'), node => [node.name, node.value]) : [];
  return { value: field ? field.value : text, text, valueText: this.getAttribute('aria-valuetext'),
    activeOption: this.getAttribute('aria-activedescendant'), hiddenInputs,
    editable, autocomplete: editable && (autocomplete === 'list' || autocomplete === 'both'), chips };
}";

const RESTORE_VALUE: &str = r"function(value) {
  const view = this.ownerDocument.defaultView;
  const prototype = this.localName === 'input' ? view.HTMLInputElement.prototype :
    this.localName === 'textarea' ? view.HTMLTextAreaElement.prototype : null;
  if (prototype && this.value !== value) {
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(this, value);
    return true;
  }
  if (this.isContentEditable && this.innerText.trim() !== value) {
    this.innerText = value;
    return true;
  }
  return false;
}";

const OPTION_SELECTED: &str = r"function() {
  const popup = this.closest('[role=listbox], [role=grid], [role=tree], [role=menu]');
  return this.isConnected && popup?.getAttribute('aria-multiselectable') === 'true'
    ? this.getAttribute('aria-selected') === 'true' : null;
}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::act) struct ComboboxState {
    pub value: String,
    pub text: String,
    pub value_text: String,
    active_option: Option<String>,
    hidden_inputs: Vec<(String, String)>,
    chips: Vec<Vec<String>>,
    pub(super) editable: bool,
    pub(super) autocomplete: bool,
}

impl ComboboxState {
    fn values(&self) -> [&str; 3] {
        [&self.value, &self.text, &self.value_text]
    }

    pub(in crate::act) fn change_from<'a>(&'a self, before: &'a Self) -> Option<&'a str> {
        self.values()
            .into_iter()
            .zip(before.values())
            .find_map(|(value, old)| (value != old).then_some(value))
            .or_else(|| {
                (self.chips != before.chips).then(|| {
                    self.new_chip_labels(before)
                        .next()
                        .or_else(|| self.chips.iter().flatten().next().map(String::as_str))
                        .unwrap_or(&self.value)
                })
            })
    }

    fn new_chip_labels<'a>(&'a self, before: &'a Self) -> impl Iterator<Item = &'a str> {
        self.chips
            .iter()
            .enumerate()
            .filter(|&(index, chip)| {
                self.chips[..index]
                    .iter()
                    .filter(|old| *old == chip)
                    .count()
                    >= before.chips.iter().filter(|old| *old == chip).count()
            })
            .flat_map(|(_, chip)| chip.iter().map(String::as_str))
    }

    pub(super) fn has_matching_chip(&self, option_texts: &[String]) -> bool {
        self.chips
            .iter()
            .flatten()
            .any(|chip| matches_option(chip, option_texts))
    }

    pub(in crate::act) fn matches(&self, option_texts: &[String]) -> bool {
        self.values()
            .into_iter()
            .any(|text| matches_option(text, option_texts))
            || self.has_matching_chip(option_texts)
    }

    pub(in crate::act) fn changed_to_match(&self, before: &Self, option_texts: &[String]) -> bool {
        (self.value != before.value && self.value_matches(option_texts))
            || self.changed_beyond_value_to_match(before, option_texts)
    }

    pub(in crate::act) fn value_matches(&self, option_texts: &[String]) -> bool {
        matches_option(&self.value, option_texts)
    }

    pub(in crate::act) fn shows_commit_beyond_value(&self, before: &Self) -> bool {
        self.hidden_inputs != before.hidden_inputs
            || (before.active_option.is_some() && self.active_option.is_none())
    }

    pub(in crate::act) fn changed_beyond_value_to_match(
        &self,
        before: &Self,
        option_texts: &[String],
    ) -> bool {
        (self.text != before.text && matches_option(&self.text, option_texts))
            || (self.value_text != before.value_text
                && matches_option(&self.value_text, option_texts))
            || self
                .new_chip_labels(before)
                .any(|chip| matches_option(chip, option_texts))
    }
}

fn matches_option(text: &str, options: &[String]) -> bool {
    let text = normalize(text);
    !text.is_empty() && options.contains(&text)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::act) struct ComboboxEvidence {
    pub previous: ComboboxState,
    pub before_pick: ComboboxState,
    pub typed: Option<String>,
    pub selected: bool,
    pub already_selected: bool,
    pub uncommitted: bool,
}

pub(in crate::act) fn option_selected(
    connection: &mut CdpConnection,
    session: &PageSession,
    option: &ElementToken,
    deadline: Deadline,
) -> Option<bool> {
    call(connection, session, option, OPTION_SELECTED, deadline)
        .ok()?
        .as_bool()
}

pub(in crate::act) fn read(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    deadline: Deadline,
) -> Result<ComboboxState, AppError> {
    let result = call(connection, session, combo, READ_COMBOBOX, deadline)?;
    let value = result["value"]
        .as_str()
        .ok_or_else(|| stale(combo, "the combobox is no longer in its document"))?;
    Ok(ComboboxState {
        value: value.to_owned(),
        text: result["text"].as_str().unwrap_or_default().to_owned(),
        value_text: result["valueText"].as_str().unwrap_or_default().to_owned(),
        active_option: result["activeOption"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned),
        hidden_inputs: serde_json::from_value(result["hiddenInputs"].clone()).unwrap_or_default(),
        chips: result["chips"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_array)
            .map(|chip| {
                chip.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .collect(),
        editable: result["editable"] == true,
        autocomplete: result["autocomplete"] == true,
    })
}

/// Restores the field without firing another input event on the widget.
pub(super) fn restore_value(
    connection: &mut CdpConnection,
    session: &PageSession,
    combo: &ElementToken,
    value: &str,
    deadline: Deadline,
) -> Result<bool, AppError> {
    call_on_element(
        connection,
        session,
        combo,
        ElementCall {
            function: RESTORE_VALUE,
            arguments: vec![json!(value)],
        },
        deadline,
    )
    .map(|reply| reply == true)
}
