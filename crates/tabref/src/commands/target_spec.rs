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
        if trimmed.contains(['"', '\'']) {
            return Self::from_words(&split_words(trimmed)?);
        }
        Locator::from_words(&[Word::quoted(trimmed)]).map(Self::Find)
    }
}

/// Whether `text` is written as a ref: `@s8f3k2p9:e4`, or `e4`.
pub(crate) fn looks_like_ref(text: &str) -> bool {
    RefId::looks_like_ref(text) || is_short_ref(text)
}

#[cfg(test)]
#[path = "target_spec_tests.rs"]
mod tests;
