use super::role_words::field_roles;

const CHECK_ROLES: &[&str] = &["checkbox", "radiobutton", "switch", "menuitem"];
const SELECT_ROLES: &[&str] = &["combobox", "listbox"];

/// What the located element is for, which narrows the roles a name may
/// match: a fill reaches only fields, a select only selects, a check only
/// checkable controls; a click, hover or read reaches any control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocateIntent {
    Click,
    Fill,
    Select,
    Check,
    Hover,
    Read,
}

impl LocateIntent {
    pub(crate) fn accepts(self, role: &str) -> bool {
        match self {
            Self::Fill => field_roles().contains(&role),
            Self::Select => SELECT_ROLES.contains(&role),
            Self::Check => CHECK_ROLES.contains(&role),
            Self::Click | Self::Hover | Self::Read => true,
        }
    }

    /// Whether text the page shows may stand for the control around it, as
    /// `click "Books set in winter"` clicks the row's link.
    pub(crate) fn reaches_text(self) -> bool {
        matches!(self, Self::Click | Self::Hover | Self::Read)
    }

    /// The control kind an error names: `field`, `select`, `checkbox` or
    /// `element`.
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Fill => "field",
            Self::Select => "select",
            Self::Check => "checkbox",
            Self::Click | Self::Hover | Self::Read => "element",
        }
    }
}
