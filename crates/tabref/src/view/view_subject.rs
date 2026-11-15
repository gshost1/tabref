use tabref_page::{PageInfo, SkippedFrame};

/// What a page view is printed about: the page's address and title, the
/// snapshot its refs belong to, how many refs it holds, and the frames the
/// snapshot left out.
pub(crate) struct ViewSubject<'a> {
    pub(crate) info: &'a PageInfo,
    pub(crate) snapshot_id: Option<&'a str>,
    pub(crate) ref_count: usize,
    pub(crate) skipped_frames: &'a [SkippedFrame],
}
