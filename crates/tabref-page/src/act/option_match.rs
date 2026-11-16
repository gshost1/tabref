/// One `<option>` of a select as the page has it: its label, its own text,
/// its value, and whether it can be picked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OptionEntry {
    pub label: String,
    pub text: String,
    pub value: String,
    pub disabled: bool,
}

/// What a choice names among a select's options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionPick {
    Chosen(usize),
    Ambiguous(Vec<usize>),
    Disabled(usize),
    Missing,
}

pub(crate) fn pick_option(_options: &[OptionEntry], _choice: &str) -> OptionPick {
    OptionPick::Missing
}

pub(crate) fn printed(option: &OptionEntry) -> String {
    option.label.clone()
}

pub(crate) fn listing(_options: &[OptionEntry], _shown: &[usize]) -> String {
    String::new()
}

#[cfg(test)]
#[path = "option_match_tests.rs"]
mod tests;
