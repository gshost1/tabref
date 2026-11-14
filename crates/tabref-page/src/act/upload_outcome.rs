/// What a file input holds after an upload: the file names it reports, and
/// whether they are the files that were set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadOutcome {
    pub files: Vec<String>,
    pub verified: bool,
}
