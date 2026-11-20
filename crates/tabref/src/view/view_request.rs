use tabref_core::AppError;

use super::detail::Detail;
use super::row_window::RowWindow;
use super::view_options::ViewOptions;
use super::view_route::{ViewRoute, quoted_argument};

/// What `snap` or `read` was asked for: chrome whole (`--all`), lines whole
/// (`--full`), one element (`--root REF`, or `read REF`), a row window
/// (`--rows`), the line to start at (`--from`) and the last one to print
/// (`--to`), and a section (`read --section`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ViewRequest {
    pub(crate) all: bool,
    pub(crate) full: bool,
    pub(crate) root: Option<String>,
    pub(crate) rows: Option<String>,
    pub(crate) from: usize,
    pub(crate) to: Option<usize>,
    pub(crate) section: Option<String>,
}

impl ViewRequest {
    /// The options for `tabref snap` on a page at `page_url`.
    pub(crate) fn snap_options(&self, page_url: &str) -> Result<ViewOptions, AppError> {
        let mut options = ViewOptions::snap(page_url, self.all, self.window()?);
        if self.full {
            options.detail = Detail::Full;
        }
        let mut command = "tabref snap".to_owned();
        if let Some(root) = &self.root {
            command.push_str(&format!(" --root {root}"));
        }
        if self.all {
            command.push_str(" --all");
        }
        options.route = ViewRoute::new(command, self.from).ending_at(self.last_line()?);
        Ok(options)
    }

    /// The options for `tabref read` on a page at `page_url`.
    pub(crate) fn read_options(&self, page_url: &str) -> Result<ViewOptions, AppError> {
        let mut options = ViewOptions::read(page_url);
        options.rows = self.window()?;
        let mut command = "tabref read".to_owned();
        if let Some(root) = &self.root {
            command.push_str(&format!(" {root}"));
        }
        if let Some(section) = &self.section {
            command.push_str(&format!(" --section {}", quoted_argument(section)));
        }
        options.route = ViewRoute::new(command, self.from).ending_at(self.last_line()?);
        Ok(options)
    }

    /// The line `--to` names, refused when it lies before `--from`.
    fn last_line(&self) -> Result<Option<usize>, AppError> {
        match self.to {
            Some(to) if to < self.from.max(1) => Err(AppError::invalid_args(format!(
                "--to {to} is before --from {}",
                self.from.max(1)
            ))
            .with_suggestion(
                "Name the first line with --from and the last with --to: --from 1 --to 40",
            )),
            to => Ok(to),
        }
    }

    fn window(&self) -> Result<RowWindow, AppError> {
        Ok(self
            .rows
            .as_deref()
            .map(RowWindow::parse)
            .transpose()?
            .unwrap_or_default())
    }
}
