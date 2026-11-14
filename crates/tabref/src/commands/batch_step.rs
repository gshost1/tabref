use tabref_page::Word;

/// One step of a `do` batch: its words, the text it was written as, and the
/// line it started on, for the step's result line and its errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BatchStep {
    pub(crate) words: Vec<Word>,
    pub(crate) source: String,
    pub(crate) line: usize,
}

impl BatchStep {
    /// The step's verb, lowered.
    pub(crate) fn verb(&self) -> String {
        self.words
            .first()
            .map(|word| word.text.to_ascii_lowercase())
            .unwrap_or_default()
    }

    /// The words after the verb.
    pub(crate) fn args(&self) -> &[Word] {
        self.words.get(1..).unwrap_or_default()
    }
}
