use tabref_core::{AppError, RefId};
use tabref_page::{Locator, Word};

use super::step_words::split_words;
use crate::session::ref_target::is_short_ref;

/// What an action is aimed at: a ref from a snapshot (`@s8f3k2p9:e4`, or
/// `e4` for the current tab's latest snapshot), or a locator resolved
/// against the page when the action runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetSpec {
    Ref(String),
    Find(Locator),
}

impl TargetSpec {
    /// A target written as words of a step: one unquoted ref, or a locator.
    pub(crate) fn from_words(words: &[Word]) -> Result<Self, AppError> {
        if let [only] = words
            && !only.quoted
            && looks_like_ref(&only.text)
        {
            return Ok(Self::Ref(only.text.clone()));
        }
        Locator::from_words(words).map(Self::Find)
    }

    /// A target given as one command-line argument. Text without quotes is
    /// one name, so `tabref click Add to cart` needs no inner quoting; text
    /// with quotes is read as words, as in `tabref click 'button "Save"'`.
    pub(crate) fn from_arg(text: &str) -> Result<Self, AppError> {
        let trimmed = text.trim();
        if looks_like_ref(trimmed) {
            return Ok(Self::Ref(trimmed.to_owned()));
        }
        if has_target_grammar(trimmed) {
            return Self::from_words(&split_words(trimmed)?);
        }
        Locator::from_words(&[Word::quoted(trimmed)]).map(Self::Find)
    }
}

pub(crate) fn has_target_grammar(text: &str) -> bool {
    text.char_indices().any(|(index, c)| {
        matches!(c, '"' | '\'') && (index == 0 || text[..index].ends_with(char::is_whitespace))
    })
}

/// Whether a target reads as a CSS selector (`input[type=file]`, `#id`,
/// `.class`, `form > input`, `a.button`), which no target accepts: targets
/// are refs or visible names. Used only to explain a target that matched
/// nothing, since a visible name may look like this too.
pub(crate) fn looks_like_css(text: &str) -> bool {
    let text = text.trim();
    let one_word = !text.contains(char::is_whitespace);
    let ident = |c: char| c.is_ascii_alphabetic() || c == '-' || c == '_';
    let mut chars = text.chars();
    let lead = chars.next();
    let second = chars.next();
    if one_word && matches!(lead, Some('#' | '.')) && second.is_some_and(ident) {
        return true;
    }
    if text.contains('[') && text.ends_with(']') {
        return true;
    }
    if text.contains(" > ") || text.contains("::") || text.contains(":nth-") {
        return true;
    }
    let tag_len = text
        .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .unwrap_or(text.len());
    one_word
        && tag_len > 0
        && text[tag_len..].starts_with(['.', '#'])
        && text[tag_len + 1..].starts_with(ident)
}

/// Whether `text` is written as a ref: `@s8f3k2p9:e4`, or `e4`.
pub(crate) fn looks_like_ref(text: &str) -> bool {
    RefId::looks_like_ref(text) || is_short_ref(text)
}

#[cfg(test)]
#[path = "target_spec_tests.rs"]
mod tests;
