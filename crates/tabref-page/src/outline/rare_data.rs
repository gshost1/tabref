use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::capture_strings::Strings;
use super::form_state::FormState;

/// The sparse per-node columns of a DOM capture: input values, checked
/// inputs, selected options, iframe documents and pseudo-elements.
pub(crate) struct RareData {
    input_values: HashMap<usize, String>,
    checked: HashSet<usize>,
    selected: HashSet<usize>,
    content_documents: HashMap<usize, usize>,
    pseudo: HashSet<usize>,
}

impl RareData {
    pub(crate) fn read(nodes: &Value, strings: &Strings<'_>) -> Self {
        let input_values = rare_pairs(nodes.get("inputValue"))
            .filter_map(|(index, value)| Some((index, strings.get(value)?.to_owned())))
            .collect();
        let content_documents = rare_pairs(nodes.get("contentDocumentIndex"))
            .filter_map(|(index, value)| Some((index, usize::try_from(value.as_u64()?).ok()?)))
            .collect();
        Self {
            input_values,
            checked: rare_indexes(nodes.get("inputChecked")),
            selected: rare_indexes(nodes.get("optionSelected")),
            content_documents,
            pseudo: rare_pairs(nodes.get("pseudoType"))
                .map(|(index, _)| index)
                .collect(),
        }
    }

    pub(crate) fn form(&self, index: usize) -> FormState {
        FormState {
            input_value: self.input_values.get(&index).cloned(),
            checked: self.checked.contains(&index),
            option_selected: self.selected.contains(&index),
            content_document: self.content_documents.get(&index).copied(),
            pseudo: self.pseudo.contains(&index),
        }
    }
}

fn rare_indexes(data: Option<&Value>) -> HashSet<usize> {
    data.and_then(|data| data.get("index"))
        .and_then(Value::as_array)
        .map(|indexes| {
            indexes
                .iter()
                .filter_map(Value::as_u64)
                .filter_map(|index| usize::try_from(index).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn rare_pairs(data: Option<&Value>) -> impl Iterator<Item = (usize, &Value)> {
    let indexes = data
        .and_then(|data| data.get("index"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let values = data
        .and_then(|data| data.get("value"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    indexes.iter().zip(values).filter_map(|(index, value)| {
        let index = usize::try_from(index.as_u64()?).ok()?;
        Some((index, value))
    })
}
