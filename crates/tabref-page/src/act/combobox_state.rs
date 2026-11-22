use serde_json::Value;
use tabref_core::{AppError, Deadline};

use super::combobox::call;
use crate::act::element_errors::stale;
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
  return { value: field ? field.value : text, text, valueText: this.getAttribute('aria-valuetext'),
    editable, autocomplete: editable && (autocomplete === 'list' || autocomplete === 'both'), chips };
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
        self.values()
            .into_iter()
            .zip(before.values())
            .any(|(text, old)| text != old && matches_option(text, option_texts))
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
    pub before_query: Option<ComboboxState>,
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
