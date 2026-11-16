/// What a select chose: the select's new value, the label of the option
/// picked, the label of the option chosen before, when one was, and whether
/// the value was already read back from the control (an ARIA combobox's
/// displayed value), so no other read is needed to trust it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectOutcome {
    pub value: String,
    pub label: String,
    pub previous: Option<String>,
    pub read_back: bool,
}
