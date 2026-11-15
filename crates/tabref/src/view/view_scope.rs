/// Which part of the page a view prints: all of it, the subtree of one
/// element (by the target and backend node id its token names), the main
/// content, or the section under the first heading containing a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewScope {
    Page,
    Element { target_id: String, backend_id: i64 },
    Main,
    Section(String),
}
