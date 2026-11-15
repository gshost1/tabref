/// One `<option>` of a select: its text, its value, and whether it is the
/// selected one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    pub selected: bool,
}
