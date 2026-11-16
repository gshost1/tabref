use super::dialog_choice::DialogChoice;

/// How a connection answers the JavaScript dialogs that open while it
/// waits: at all (`auto`, the default), and, for the next one only, the
/// answer a command chose (`next`); every other dialog is accepted.
#[derive(Debug, Clone)]
pub(crate) struct DialogAnswers {
    pub(crate) auto: bool,
    pub(crate) next: Option<DialogChoice>,
}

impl Default for DialogAnswers {
    fn default() -> Self {
        Self {
            auto: true,
            next: None,
        }
    }
}
