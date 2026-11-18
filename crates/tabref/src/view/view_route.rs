/// The command that printed a view, as its hints repeat it (`tabref snap
/// --root @s1:e5`, `tabref read --section "History"`), without `--rows`,
/// `--from`, `--to` or `--full`, the view line the output starts at, and
/// the last line it may print when `--to` named one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewRoute {
    pub(crate) command: String,
    pub(crate) from: usize,
    pub(crate) to: Option<usize>,
}

impl ViewRoute {
    pub(crate) fn new(command: impl Into<String>, from: usize) -> Self {
        Self {
            command: command.into(),
            from: from.max(1),
            to: None,
        }
    }

    /// The same route, printing no line after `to`.
    pub(crate) fn ending_at(mut self, to: Option<usize>) -> Self {
        self.to = to;
        self
    }

    /// The command that prints the view from line `line` on.
    pub(crate) fn starting_at(&self, line: usize, full: bool) -> String {
        let full = if full { " --full" } else { "" };
        format!("{}{full} --from {line}", self.command)
    }

    /// The command that prints rows `first` onwards of long tables.
    pub(crate) fn rows_from(&self, first: usize, full: bool) -> String {
        let full = if full { " --full" } else { "" };
        format!("{}{full} --rows {first}-", self.command)
    }
}

impl Default for ViewRoute {
    fn default() -> Self {
        Self::new("tabref snap", 1)
    }
}

/// `text` as a quoted command argument, its quotes and backslashes escaped.
pub(crate) fn quoted_argument(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
