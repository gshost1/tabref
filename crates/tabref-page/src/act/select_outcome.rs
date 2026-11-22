use serde_json::json;
use tabref_core::{AppError, Deadline};

use super::select_option::combobox_popup_report::PopupText;
use super::select_option::combobox_popup_report::omit_display_change;
use super::select_option::combobox_state;
use crate::tree::ax_nodes::AxNodes;
use crate::{CdpConnection, ElementToken, PageChanges, PageMark, PageSession};

/// What a select chose: the native select's new value, the label picked,
/// and the prior native label or combobox value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectOutcome {
    pub value: String,
    pub label: String,
    pub previous: Option<String>,
    pub combobox: bool,
    pub(crate) option_texts: Vec<String>,
    pub(in crate::act) readback: Option<combobox_state::ComboboxEvidence>,
    pub(in crate::act) popup_text: Option<PopupText>,
    pub popup_error: Option<String>,
    pub(crate) previous_text: String,
}

pub struct ComboboxReadback {
    pub value: String,
    pub text: String,
    pub kept: bool,
    pub changed_to: Option<String>,
}

impl SelectOutcome {
    /// Reads the combobox after settle and checks its value and displayed text against the option.
    pub fn read_combobox_value(
        &self,
        connection: &mut CdpConnection,
        session: &PageSession,
        token: &ElementToken,
        deadline: Deadline,
    ) -> Result<ComboboxReadback, AppError> {
        let state = combobox_state::read(connection, session, token, deadline)?;
        let evidence = self
            .readback
            .as_ref()
            .ok_or_else(|| AppError::internal("combobox selection has no read-back baseline"))?;
        let kept = evidence.selected
            || (!evidence.uncommitted
                && (state.matches(&evidence.before_pick, &self.option_texts)
                    || evidence
                        .before_query
                        .as_ref()
                        .is_some_and(|before| state.matches(before, &self.option_texts))))
            || combobox_state::option_selected(
                connection,
                session,
                &evidence.option_token,
                deadline,
            );
        let changed_to = if evidence.uncommitted {
            None
        } else {
            state
                .change_from(&evidence.before_pick)
                .or_else(|| {
                    evidence
                        .before_query
                        .as_ref()
                        .and_then(|before| state.change_from(before))
                })
                .map(str::to_owned)
        };
        Ok(ComboboxReadback {
            value: state.value,
            text: state.text,
            kept,
            changed_to,
        })
    }

    /// Excludes the chosen popup's former text from the page change baseline.
    pub fn omit_popup_from(&self, mark: &mut PageMark) {
        if let Some(popup) = &self.popup_text {
            popup.omit_from(mark);
        }
    }

    /// Omits a rewrite of the combobox's own display, already covered by the selected line.
    pub fn omit_combobox_display_change(&self, text: &str, page: &mut PageChanges) {
        omit_display_change(&self.previous_text, text, &mut page.changes);
    }

    /// The browser may expose an editable combobox's value as a separate text node.
    pub fn combobox_value_text_tokens(
        connection: &mut CdpConnection,
        session: &PageSession,
        token: &ElementToken,
        value: &str,
        deadline: Deadline,
    ) -> Vec<String> {
        let Ok(reply) = connection.call(
            "Accessibility.queryAXTree",
            json!({ "backendNodeId": token.backend_node_id }),
            Some(&session.session_id),
            deadline,
        ) else {
            return Vec::new();
        };
        let Ok(nodes) = serde_json::from_value::<AxNodes>(reply) else {
            return Vec::new();
        };
        nodes
            .nodes
            .into_iter()
            .filter_map(|node| {
                if node.ignored || node.raw_role() != "StaticText" {
                    return None;
                }
                let text = node.text.name?.as_text()?;
                (text == value)
                    .then_some(ElementToken {
                        backend_node_id: node.backend_dom_node_id?,
                        ..token.clone()
                    })
                    .map(|token| token.to_string())
            })
            .collect()
    }
}
