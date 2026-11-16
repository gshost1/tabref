/// A watch, started in the page right after a focus, on whether the page
/// moves the focus away within a short window: the promise the page holds
/// it as, or `None` when nothing on the element's path listens for focus
/// events, so nothing is watched. [`focus_kept`](crate::focus_kept) reads
/// it.
#[derive(Debug)]
pub struct FocusWatch {
    pub(crate) promise: Option<String>,
}
