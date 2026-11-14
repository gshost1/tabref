use super::ref_style::RefStyle;
use super::row_window::RowWindow;

/// How a page view is printed: as a snapshot (controls with refs and
/// addresses) or as reading text, its budget in characters, the rows of
/// long tables and lists to show, whether site chrome (navigation, header,
/// footer, sidebars) is printed whole, the page's address for shortening
/// links, and how refs print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewOptions {
    pub(crate) reading: bool,
    pub(crate) budget: usize,
    pub(crate) rows: RowWindow,
    pub(crate) whole_chrome: bool,
    pub(crate) page_url: String,
    pub(crate) refs: RefStyle,
}

/// About 3.5k tokens: a dense page's main content and its outline fit, a
/// simple page is far under it.
pub(crate) const SNAP_BUDGET: usize = 14_000;
pub(crate) const ALL_BUDGET: usize = 80_000;
pub(crate) const READ_BUDGET: usize = 40_000;

impl ViewOptions {
    pub(crate) fn snap(page_url: &str, all: bool, rows: RowWindow) -> Self {
        Self {
            reading: false,
            budget: if all { ALL_BUDGET } else { SNAP_BUDGET },
            rows,
            whole_chrome: all,
            page_url: page_url.to_owned(),
            refs: RefStyle::from_env(),
        }
    }

    pub(crate) fn read(page_url: &str) -> Self {
        Self {
            reading: true,
            budget: READ_BUDGET,
            rows: RowWindow::default(),
            whole_chrome: true,
            page_url: page_url.to_owned(),
            refs: RefStyle::from_env(),
        }
    }
}
