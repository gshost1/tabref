use tabref_core::AppError;

/// What `wait` waits for.
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
        named.extend(url.map(|url| Self::Url(url.to_owned())));
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

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Text(text) => format!("text {text:?} to appear"),
            Self::Gone(text) => format!("text {text:?} to go"),
            Self::Url(url) => format!("the address to contain {url:?}"),
            Self::Idle => "the network to go quiet".to_owned(),
        }
    }
}

#[cfg(test)]
#[path = "wait_condition_tests.rs"]
mod tests;
