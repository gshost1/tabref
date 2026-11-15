/// What a select chose: the select's new value, the label of the option
/// picked, and the label of the option chosen before, when one was.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectOutcome {
    pub value: String,
    pub label: String,
    pub previous: Option<String>,
}
