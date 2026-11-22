use tabref_core::AppError;

/// Which rows of a long table or list a view prints, 1-based and inclusive:
/// by default the first 20, or what `--rows 21-40` or `--rows 21-` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowWindow {
    pub(crate) first: usize,
    pub(crate) last: Option<usize>,
    pub(crate) chosen: bool,
}

pub(crate) const DEFAULT_ROWS: usize = 20;
/// The most rows a `--rows` window may ask for and still print whole past
/// a budget cut.
const KEPT_ROWS: usize = 10;

impl Default for RowWindow {
    fn default() -> Self {
        Self {
            first: 1,
            last: Some(DEFAULT_ROWS),
            chosen: false,
        }
    }
}

impl RowWindow {
    /// Parses `21-40`, `21-` or `5`.
    pub(crate) fn parse(text: &str) -> Result<Self, AppError> {
        let invalid = || {
            AppError::invalid_args(format!("'{text}' is not a row range"))
                .with_suggestion("Write --rows 21-40, --rows 21- or --rows 5")
        };
        let number = |part: &str| part.trim().parse::<usize>().ok().filter(|row| *row > 0);
        let (first, last) = match text.split_once('-') {
            Some((first, "")) => (number(first).ok_or_else(invalid)?, None),
            Some((first, last)) => (
                number(first).ok_or_else(invalid)?,
                Some(number(last).ok_or_else(invalid)?),
            ),
            None => {
                let row = number(text).ok_or_else(invalid)?;
                (row, Some(row))
            }
        };
        if last.is_some_and(|last| last < first) {
            return Err(invalid());
        }
        Ok(Self {
            first,
            last,
            chosen: true,
        })
    }

    /// Whether the window was asked for and is short enough that a budget
    /// cut must still print its rows.
    pub(crate) fn kept(&self) -> bool {
        self.chosen
            && self
                .last
                .is_some_and(|last| last >= self.first && last - self.first < KEPT_ROWS)
    }

    pub(crate) fn contains(&self, row: usize) -> bool {
        row >= self.first && self.last.is_none_or(|last| row <= last)
    }
}

#[cfg(test)]
#[path = "row_window_tests.rs"]
mod tests;
