/// A watch, started in the page right after a focus, on whether the page
/// moves the focus away within a short window. The page holds it as a
/// promise; [`focus_kept`](crate::focus_kept) waits for its answer.
#[derive(Debug)]
pub struct FocusWatch {
    pub(crate) promise: String,
}
