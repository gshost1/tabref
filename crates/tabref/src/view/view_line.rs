/// One printed line of a page view: its indent depth, its text (a `<pre>`
/// block keeps its own line breaks), its level and plain text when it is a
/// heading, the chrome landmark (navigation, header, footer, sidebar) it
/// belongs to, numbered in page order, its number in the view, which
/// `--from` takes, what was cut off it (`+812 chars`, `+20 lines`), and
/// whether a budget cut still prints it (a row `--rows` asked for).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewLine {
    pub(crate) depth: usize,
    pub(crate) text: String,
    pub(crate) heading: Option<u8>,
    pub(crate) plain: Option<String>,
    pub(crate) chrome: Option<usize>,
    pub(crate) number: usize,
    pub(crate) cut: Option<String>,
    pub(crate) keep: bool,
}

impl ViewLine {
    pub(crate) fn new(depth: usize, text: impl Into<String>) -> Self {
        Self {
            depth,
            text: text.into(),
            heading: None,
            plain: None,
            chrome: None,
            number: 0,
            cut: None,
            keep: false,
        }
    }

    pub(crate) fn printed(&self) -> String {
        let indent = "  ".repeat(self.depth);
        self.text
            .split('\n')
            .map(|part| format!("{indent}{part}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The characters the line costs once printed, its line breaks included.
    pub(crate) fn cost(&self) -> usize {
        let parts = self.text.split('\n').count();
        parts * (self.depth * 2 + 1) + self.text.chars().count()
    }

    /// The heading's text as `read --section` matches it.
    pub(crate) fn heading_text(&self) -> String {
        self.plain
            .clone()
            .unwrap_or_else(|| self.text.trim_start_matches('#').trim().to_owned())
    }
}
