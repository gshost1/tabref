use tabref_page::PageInfo;

/// What a page view is printed about: the page's address and title, the
/// snapshot its refs belong to, how many refs it holds, and how many
/// frames the snapshot could not read.
pub(crate) struct ViewSubject<'a> {
    pub(crate) info: &'a PageInfo,
    pub(crate) snapshot_id: Option<&'a str>,
    pub(crate) ref_count: usize,
    pub(crate) unreadable_frames: usize,
}
