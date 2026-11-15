use super::select_option::SelectOption;

/// A control as the outline shows it: the ref the snapshot gave it, its
/// role, its accessible name, a link's absolute address, a field's current
/// value, its state tokens (`checked`, `collapsed`, `disabled`, ...), and a
/// select's options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Control {
    pub ref_text: Option<String>,
    pub role: String,
    pub name: String,
    pub href: Option<String>,
    pub value: Option<String>,
    pub states: Vec<String>,
    pub options: Vec<SelectOption>,
}

impl Control {
    pub fn is_link(&self) -> bool {
        self.role == "link"
    }

    pub fn has_state(&self, state: &str) -> bool {
        self.states.iter().any(|known| known == state)
    }
}
