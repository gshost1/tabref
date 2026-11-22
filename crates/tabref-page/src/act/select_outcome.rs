use serde_json::json;
use tabref_core::{AppError, Deadline};

use super::select_option::combobox;
use super::select_option::combobox_popup_report::PopupText;
use super::select_option::combobox_popup_report::omit_display_change;
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
    pub(crate) uncommitted: bool,
    pub(in crate::act) popup_text: Option<PopupText>,
    pub popup_error: Option<String>,
    pub(crate) previous_text: String,
}

impl SelectOutcome {
    /// Reads the combobox after settle and checks its value and displayed text against the option.
    pub fn read_combobox_value(
        &self,
        connection: &mut CdpConnection,
        session: &PageSession,
        token: &ElementToken,
        deadline: Deadline,
    ) -> Result<(String, String, bool), AppError> {
        let state = combobox::read(connection, session, token, deadline)?;
        let kept = !self.uncommitted && state.matches(&self.option_texts);
        Ok((state.value, state.text, kept))
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
