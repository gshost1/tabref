/// One printed line of a page view: its indent depth, its text, its level
/// when it is a heading, and the chrome landmark (navigation, header,
/// footer, sidebar) it belongs to, numbered in page order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewLine {
    pub(crate) depth: usize,
    pub(crate) text: String,
    pub(crate) heading: Option<u8>,
    pub(crate) chrome: Option<usize>,
}

impl ViewLine {
    pub(crate) fn printed(&self) -> String {
        format!("{}{}", "  ".repeat(self.depth), self.text)
    }

    /// The characters the line costs once printed, its line break included.
    pub(crate) fn cost(&self) -> usize {
        self.depth * 2 + self.text.chars().count() + 1
    }
}
