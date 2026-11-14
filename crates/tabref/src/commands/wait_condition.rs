use tabref_core::AppError;
use tabref_page::Word;

/// What `wait` waits for, and what a `do` batch's `expect` checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Condition {
    Text(String),
    Gone(String),
    Url(String),
    Idle,
}

impl Condition {
    /// The one condition the flags name; none, or more than one, is
    /// `INVALID_ARGS`.
    pub(crate) fn from_flags(
        text: Option<&str>,
        gone: Option<&str>,
        url: Option<&str>,
        idle: bool,
    ) -> Result<Self, AppError> {
        let mut named = Vec::new();
        named.extend(text.map(|text| Self::Text(text.to_owned())));
        named.extend(gone.map(|text| Self::Gone(text.to_owned())));
        named.extend(url.map(|url| Self::Url(url_part(url))));
        if idle {
            named.push(Self::Idle);
        }
        match named.len() {
            1 => Ok(named.remove(0)),
            _ => Err(AppError::invalid_args(
                "Name exactly one of --text, --gone, --url or --idle",
            )),
        }
    }

    /// `text "Saved"`, `gone "Loading"`, `url ~saved=1`, `idle`, or bare
    /// text, which waits for that text to show.
    pub(crate) fn from_words(words: &[Word]) -> Result<Self, AppError> {
        let joined = |rest: &[Word]| {
            rest.iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let condition = match words {
            [first, rest @ ..] if first.is_keyword("text") => Self::Text(joined(rest)),
            [first, rest @ ..] if first.is_keyword("gone") => Self::Gone(joined(rest)),
            [first, rest @ ..] if first.is_keyword("url") => Self::Url(url_part(&joined(rest))),
            [first] if first.is_keyword("idle") => Self::Idle,
            rest => Self::Text(joined(rest)),
        };
        match &condition {
            Self::Text(text) | Self::Gone(text) | Self::Url(text) if text.trim().is_empty() => Err(
                AppError::invalid_args(
                    "expect needs what to check: text \"Saved\", gone \"Loading\", url ~saved=1 or idle",
                ),
            ),
            _ => Ok(condition),
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Text(text) => format!("text {text:?} to appear"),
            Self::Gone(text) => format!("text {text:?} to go"),
            Self::Url(url) => format!("the address to contain {url:?}"),
            Self::Idle => "the network to go quiet".to_owned(),
        }
    }
}

/// The part of an address to look for: `~saved=1` and `/saved/` both look
/// for `saved=1` and `saved` inside the address.
fn url_part(text: &str) -> String {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix('~').unwrap_or(trimmed).trim();
    match trimmed
        .strip_prefix('/')
        .and_then(|inner| inner.strip_suffix('/'))
    {
        Some(inner) if !inner.is_empty() && !inner.contains('/') => inner.to_owned(),
        _ => trimmed.to_owned(),
    }
}

#[cfg(test)]
#[path = "wait_condition_tests.rs"]
mod tests;
