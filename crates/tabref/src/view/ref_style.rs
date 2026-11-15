/// How a view prints refs. `Long` is the snapshot-qualified `@s8f3k2p9:e4`
/// every command accepts. `Short` is `e4`, which names element 4 of the
/// tab's latest snapshot; the header then says which snapshot that is.
/// Short is the default; `TABREF_REFS=long` picks the long form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefStyle {
    Long,
    Short,
}

impl RefStyle {
    pub(crate) fn from_env() -> Self {
        match std::env::var("TABREF_REFS").as_deref() {
            Ok("long") => Self::Long,
            _ => Self::Short,
        }
    }

    /// `ref_id` (`@s8f3k2p9:e4`) as this style prints it.
    pub(crate) fn print(self, ref_id: &str) -> String {
        match self {
            Self::Long => ref_id.to_owned(),
            Self::Short => ref_id
                .rsplit_once(':')
                .map_or_else(|| ref_id.to_owned(), |(_, element)| element.to_owned()),
        }
    }
}

#[cfg(test)]
#[path = "ref_style_tests.rs"]
mod tests;
