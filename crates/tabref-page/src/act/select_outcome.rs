use tabref_core::{AppError, Deadline};

use super::select_option::combobox;
use crate::{CdpConnection, ElementToken, PageSession};

/// What a select chose: the native select's new value, the label picked,
/// and the prior native label or combobox value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectOutcome {
    pub value: String,
    pub label: String,
    pub previous: Option<String>,
    pub combobox: bool,
}

impl SelectOutcome {
    /// Reads the combobox's value or displayed text after the pick settled.
    pub fn read_combobox_value(
        connection: &mut CdpConnection,
        session: &PageSession,
        token: &ElementToken,
        deadline: Deadline,
    ) -> Result<String, AppError> {
        combobox::read(connection, session, token, deadline).map(|(value, _)| value)
    }
}
