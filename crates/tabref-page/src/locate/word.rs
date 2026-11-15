/// One word of a step or target, as the step's text gave it: quoted words
/// are names and values and never keywords, so `click "link"` names a link
/// text while `click link "Docs"` names a role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    pub quoted: bool,
}

impl Word {
    pub fn bare(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            quoted: false,
        }
    }

    pub fn quoted(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            quoted: true,
        }
    }

    /// Whether the word is the unquoted keyword `keyword`, ignoring case.
    pub fn is_keyword(&self, keyword: &str) -> bool {
        !self.quoted && self.text.eq_ignore_ascii_case(keyword)
    }
}
