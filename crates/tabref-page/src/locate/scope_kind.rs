/// How a target's scope text places it: `in` the row or region that holds
/// the text, or the nearest match `after` or `before` the text in reading
/// order, for pages that lay records out as plain paragraphs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScopeKind {
    #[default]
    Within,
    After,
    Before,
}

impl ScopeKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Within => "in",
            Self::After => "after",
            Self::Before => "before",
        }
    }
}
