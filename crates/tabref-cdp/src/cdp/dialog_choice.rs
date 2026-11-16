/// How to answer a JavaScript dialog (`alert`, `confirm`, `prompt`,
/// `beforeunload`): accept it, with the text a prompt gets (its own default
/// when `None`), or dismiss it (Cancel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogChoice {
    Accept { text: Option<String> },
    Dismiss,
}

impl DialogChoice {
    /// The choice a `--dialog` value names: `accept`, `accept=TEXT` (the
    /// text a prompt gets, which may be empty) or `dismiss`.
    pub fn parse(text: &str) -> Option<Self> {
        match text.split_once('=') {
            Some(("accept", answer)) => Some(Self::Accept {
                text: Some(answer.to_owned()),
            }),
            None if text == "accept" => Some(Self::Accept { text: None }),
            None if text == "dismiss" => Some(Self::Dismiss),
            _ => None,
        }
    }

    /// Whether this choice answers a dialog of `kind`: never an `alert`,
    /// which asks nothing (so a confirm after it still gets the choice);
    /// `accept=TEXT` only a `prompt`; `accept` and `dismiss` a `confirm`,
    /// `prompt` or `beforeunload`.
    pub fn applies_to(&self, kind: &str) -> bool {
        match self {
            Self::Accept { text: Some(_) } => kind == "prompt",
            _ => matches!(kind, "confirm" | "prompt" | "beforeunload"),
        }
    }

    /// Whether the dialog is accepted, and the text a prompt gets, given the
    /// prompt's own default.
    pub(crate) fn answer(&self, default_prompt: &str) -> (bool, String) {
        match self {
            Self::Accept { text } => (true, text.as_deref().unwrap_or(default_prompt).to_owned()),
            Self::Dismiss => (false, String::new()),
        }
    }
}

#[cfg(test)]
#[path = "dialog_choice_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dialog_tab_tests.rs"]
mod tab_tests;
