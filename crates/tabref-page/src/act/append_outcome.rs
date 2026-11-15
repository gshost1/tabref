/// What an append left in the field: its whole content before and after,
/// and whether the after is the before with the typed text at its end. A
/// page may truncate (`maxlength`), reformat or reject input, so `verified`
/// can be false after a delivered edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendOutcome {
    pub previous: String,
    pub observed: String,
    pub verified: bool,
}
