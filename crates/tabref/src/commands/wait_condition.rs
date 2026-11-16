use tabref_core::AppError;
use tabref_page::{ElementState, Word};

use super::target_spec::{TargetSpec, looks_like_ref};
use super::wait_flags::WaitFlags;

/// What `wait` waits for, and what a `do` batch's `expect` checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Condition {
    Text(String),
    Gone(String),
    Url(String),
    Idle,
    Download,
    Element {
        target: TargetSpec,
        label: String,
        state: ElementState,
    },
}

impl Condition {
    /// The one condition the flags name; none, or more than one, is
    /// `INVALID_ARGS`.
    pub(crate) fn from_flags(flags: &WaitFlags<'_>) -> Result<Self, AppError> {
        let mut named = Vec::new();
        named.extend(flags.text.map(|text| Self::Text(text.to_owned())));
        named.extend(flags.gone.map(|text| Self::Gone(text.to_owned())));
        named.extend(flags.url.map(|url| Self::Url(url_part(url))));
        if flags.idle {
            named.push(Self::Idle);
        }
        if flags.download {
            named.push(Self::Download);
        }
        match (flags.target, flags.state) {
            (Some(target), Some(state)) => named.push(Self::element(
                TargetSpec::from_arg(target)?,
                target.to_owned(),
                state,
            )?),
            (None, None) => {}
            _ => {
                return Err(AppError::invalid_args(
                    "--ref and --state go together, as in: wait --ref \"Save\" --state enabled",
                ));
            }
        }
        match named.len() {
            1 => Ok(named.remove(0)),
            _ => Err(AppError::invalid_args(
                "Name exactly one of --text, --gone, --url, --idle, --download or --ref with --state",
            )),
        }
    }

    /// `text "Saved"`, `gone "Loading"`, `url ~saved=1`, `idle`, `download`,
    /// `T is S` with T one unquoted ref or words that hold a quoted name
    /// (`"Save" is enabled`, `button "Save" is enabled`, `e4 is "value=62
    /// 00"` quoted whole when it has spaces), or
    /// bare text, which waits for that text to show. A keyword comes first,
    /// so `text Notifications is enabled` waits for that text.
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
            [first] if first.is_keyword("download") => Self::Download,
            [target @ .., is, state] if is.is_keyword("is") && names_element(target) => {
                return Self::element(TargetSpec::from_words(target)?, label(target), &state.text);
            }
            rest if unquoted_value(rest) => {
                return Err(AppError::invalid_args(
                    "quote a value with spaces, as in: expect \"Total\" is \"value=62 00\"",
                ));
            }
            rest => Self::Text(joined(rest)),
        };
        match &condition {
            Self::Text(text) | Self::Gone(text) | Self::Url(text) if text.trim().is_empty() => {
                Err(AppError::invalid_args(
                    "expect needs what to check: text \"Saved\", gone \"Loading\", url ~saved=1 or idle",
                ))
            }
            _ => Ok(condition),
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Text(text) => format!("text {text:?} to appear"),
            Self::Gone(text) => format!("text {text:?} to go"),
            Self::Url(url) => format!("the address to contain {url:?}"),
            Self::Idle => "the network to go quiet".to_owned(),
            Self::Download => "a download to finish".to_owned(),
            Self::Element { label, state, .. } => format!("{label} to be {}", state.describe()),
        }
    }
}

impl Condition {
    fn element(target: TargetSpec, label: String, state: &str) -> Result<Self, AppError> {
        let state = ElementState::parse(state).ok_or_else(|| {
            AppError::invalid_args(format!(
                "'{state}' is not a state; give enabled, visible, hidden, gone or value=X"
            ))
        })?;
        Ok(Self::Element {
            target,
            label,
            state,
        })
    }
}

/// Whether `target` names an element: one unquoted ref, or words that hold
/// a quoted name (`"Save"`, `button "Save"`).
fn names_element(target: &[Word]) -> bool {
    match target {
        [] => false,
        [only] if !only.quoted => looks_like_ref(&only.text),
        words => words.iter().any(|word| word.quoted),
    }
}

/// Whether `words` are an element condition whose `value=` was split at a
/// space: `T is value=a b`.
fn unquoted_value(words: &[Word]) -> bool {
    (1..words.len().saturating_sub(2)).any(|at| {
        words[at].is_keyword("is") && names_element(&words[..at]) && is_value(&words[at + 1].text)
    })
}

fn label(target: &[Word]) -> String {
    target
        .iter()
        .map(|word| {
            if word.quoted {
                format!("{:?}", word.text)
            } else {
                word.text.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_value(state: &str) -> bool {
    state
        .get(..6)
        .is_some_and(|name| name.eq_ignore_ascii_case("value="))
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
