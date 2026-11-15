use super::detail::Detail;
use super::ref_style::RefStyle;
use super::row_window::RowWindow;
use super::view_route::ViewRoute;

/// How a page view is printed: how much of each line it keeps, its budget
/// in characters, the rows of long tables and lists to show, whether site
/// chrome (navigation, header, footer, sidebars) is printed whole, the
/// page's address for shortening links, how refs print, and the command
/// its hints repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewOptions {
    pub(crate) detail: Detail,
    pub(crate) budget: usize,
    pub(crate) rows: RowWindow,
    pub(crate) whole_chrome: bool,
    pub(crate) page_url: String,
    pub(crate) refs: RefStyle,
    pub(crate) route: ViewRoute,
}

/// About 3.5k tokens of content: with its markers and the headings after
/// the cut a dense page stays near 4k, a simple page is far under it.
pub(crate) const SNAP_BUDGET: usize = 12_000;
pub(crate) const ALL_BUDGET: usize = 80_000;
pub(crate) const READ_BUDGET: usize = 24_000;

impl ViewOptions {
    pub(crate) fn snap(page_url: &str, all: bool, rows: RowWindow) -> Self {
        Self {
            detail: Detail::Snap,
            budget: if all { ALL_BUDGET } else { SNAP_BUDGET },
            rows,
            whole_chrome: all,
            page_url: page_url.to_owned(),
            refs: RefStyle::from_env(),
            route: ViewRoute::default(),
        }
    }

    pub(crate) fn read(page_url: &str) -> Self {
        Self {
            detail: Detail::Read,
            budget: READ_BUDGET,
            rows: RowWindow::default(),
            whole_chrome: true,
            page_url: page_url.to_owned(),
            refs: RefStyle::from_env(),
            route: ViewRoute::new("tabref read", 1),
        }
    }

    pub(crate) fn reading(&self) -> bool {
        self.detail == Detail::Read
    }

    /// Whether lines print whole rather than cut with a marker.
    pub(crate) fn whole_lines(&self) -> bool {
        self.detail != Detail::Snap
    }

    /// Whether hints must ask for `--full` to keep the same lines.
    pub(crate) fn asks_full(&self) -> bool {
        self.detail == Detail::Full
    }
}
