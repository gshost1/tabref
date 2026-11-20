use super::file_input_choice::FileInputVia;

/// What a file input holds after an upload: the file names it reports,
/// whether the page reports names matching the requested files, which input took them (as
/// `input#cv` or `input.dz-hidden-input`), whether a person can see that
/// input, and why it was the one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadOutcome {
    pub files: Vec<String>,
    pub page_reports_requested_files: bool,
    pub input: String,
    pub hidden: bool,
    pub via: FileInputVia,
}
