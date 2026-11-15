use tabref_core::AppError;

use super::detail::Detail;
use super::row_window::RowWindow;
use super::view_options::ViewOptions;
use super::view_route::{ViewRoute, quoted_argument};

/// What `snap` or `read` was asked for: chrome whole (`--all`), lines whole
/// (`--full`), one element (`--root REF`, or `read REF`), a row window
/// (`--rows`), the line to start at (`--from`), and a section
/// (`read --section`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ViewRequest {
    pub(crate) all: bool,
    pub(crate) full: bool,
    pub(crate) root: Option<String>,
    pub(crate) rows: Option<String>,
    pub(crate) from: usize,
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
        options.route = ViewRoute::new(command, self.from);
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
        options.route = ViewRoute::new(command, self.from);
        Ok(options)
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
