/// Why an element cannot take an action yet: it is not shown, it has no
/// size on screen to press, it can never be pressed (an image-map area), it
/// does not accept input, or it does not accept typed text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    Hidden,
    NoSize,
    Unpressable,
    Disabled,
    ReadOnly,
}

impl Blocker {
    /// The blocker the in-page check names: `hidden`, `zero`,
    /// `unpressable`, `disabled` or `readonly`.
    pub(crate) fn from_wire(text: &str) -> Option<Self> {
        match text {
            "hidden" => Some(Self::Hidden),
            "zero" => Some(Self::NoSize),
            "unpressable" => Some(Self::Unpressable),
            "disabled" => Some(Self::Disabled),
            "readonly" => Some(Self::ReadOnly),
            _ => None,
        }
    }

    /// Whether waiting cannot help: a node with no size does not grow one by
    /// waiting, and an image-map area never gets a box.
    pub(crate) fn is_settled(self) -> bool {
        matches!(self, Self::NoSize | Self::Unpressable)
    }

    /// The `details.kind` an error with this blocker carries.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Hidden => "element_hidden",
            Self::NoSize => "element_zero_size",
            Self::Unpressable => "element_not_pressable",
            Self::Disabled => "element_disabled",
            Self::ReadOnly => "element_readonly",
        }
    }
}
