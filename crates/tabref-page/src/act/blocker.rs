/// Why an element cannot take an action yet: it is not shown, it does not
/// accept input, or it does not accept typed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    Hidden,
    Disabled,
    ReadOnly,
}

impl Blocker {
    /// The blocker the in-page check names: `hidden`, `disabled` or
    /// `readonly`.
    pub(crate) fn from_wire(text: &str) -> Option<Self> {
        match text {
            "hidden" => Some(Self::Hidden),
            "disabled" => Some(Self::Disabled),
            "readonly" => Some(Self::ReadOnly),
            _ => None,
        }
    }

    /// The `details.kind` an error with this blocker carries.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Hidden => "element_hidden",
            Self::Disabled => "element_disabled",
            Self::ReadOnly => "element_readonly",
        }
    }
}
