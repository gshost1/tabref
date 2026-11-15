use tabref_core::AppError;

use super::role_words::role_word;
use super::scope_kind::ScopeKind;
use super::word::Word;

/// A target named by what the page shows rather than by a ref: an optional
/// role word, the name (label, text, or description), an optional `nth N`
/// among several matches, and an optional scope text with how it places
/// the target: `in "<text>"` keeps to the row or region around the text,
/// `after "<text>"` and `before "<text>"` take the nearest match on that
/// side of it in reading order. Resolved against the page when the action
/// runs, so it works on pages no snapshot has shown yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locator {
    pub role: Option<String>,
    pub name: Option<String>,
    pub nth: Option<usize>,
    pub scope: Option<String>,
    pub scope_kind: ScopeKind,
}

impl Locator {
    /// Reads `[role] [name] [nth N] [in|after|before TEXT]`. An unquoted first word that
    /// is a role word is the role; any other first word is the name.
    pub fn from_words(words: &[Word]) -> Result<Self, AppError> {
        let mut locator = Self::default();
        let mut rest = words;
        if let [first, tail @ ..] = rest
            && !first.quoted
            && role_word(&first.text).is_some()
        {
            locator.role = Some(first.text.to_ascii_lowercase());
            rest = tail;
        }
        while let [word, tail @ ..] = rest {
            if word.is_keyword("nth") {
                let number = tail.first().and_then(|n| n.text.parse::<usize>().ok());
                locator.nth = Some(number.filter(|n| *n > 0).ok_or_else(|| {
                    AppError::invalid_args("nth takes a number from 1, as in: click \"Edit\" nth 2")
                })?);
                rest = &tail[1..];
            } else if let Some(kind) = scope_keyword(word) {
                let scope = tail.first().ok_or_else(|| {
                    AppError::invalid_args(format!(
                        "{} takes text the page shows, as in: click \"Edit\" {} \"Blue hoodie\"",
                        kind.keyword(),
                        kind.keyword()
                    ))
                })?;
                locator.scope = Some(scope.text.clone());
                locator.scope_kind = kind;
                rest = &tail[1..];
            } else if locator.name.is_none() {
                locator.name = Some(word.text.clone());
                rest = tail;
            } else {
                return Err(AppError::invalid_args(format!(
                    "unexpected '{}' in the target; quote a name with spaces",
                    word.text
                )));
            }
        }
        if locator.role.is_none() && locator.name.is_none() {
            return Err(AppError::invalid_args(
                "the target names nothing; give a ref, a name, or a role and a name",
            ));
        }
        Ok(locator)
    }

    /// The target as a message names it: `button "Save" in "Row 3"`.
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(role) = &self.role {
            parts.push(role.clone());
        }
        if let Some(name) = &self.name {
            parts.push(format!("{name:?}"));
        }
        if let Some(nth) = self.nth {
            parts.push(format!("nth {nth}"));
        }
        if let Some(scope) = &self.scope {
            parts.push(format!("{} {scope:?}", self.scope_kind.keyword()));
        }
        parts.join(" ")
    }
}

fn scope_keyword(word: &Word) -> Option<ScopeKind> {
    [ScopeKind::Within, ScopeKind::After, ScopeKind::Before]
        .into_iter()
        .find(|kind| word.is_keyword(kind.keyword()))
}

#[cfg(test)]
#[path = "locator_tests.rs"]
mod tests;
