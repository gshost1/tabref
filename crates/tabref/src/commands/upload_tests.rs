use tabref_core::{AppError, ErrorCode};
use tabref_page::{FileInputVia, UploadOutcome};

use super::{css_explained, summary};

fn outcome(via: FileInputVia, hidden: bool) -> UploadOutcome {
    UploadOutcome {
        files: vec!["a.txt".into(), "b.txt".into()],
        page_reports_requested_files: true,
        input: "input.dz-hidden-input".into(),
        hidden,
        via,
    }
}

#[test]
fn the_line_names_a_hidden_input_found_behind_the_target() {
    assert_eq!(
        summary(
            &outcome(FileInputVia::Nearest, true),
            "\"select more files…\""
        ),
        "set files on \"select more files…\" via hidden input.dz-hidden-input (the nearest file input) (page reports: a.txt, b.txt)"
    );
}

#[test]
fn a_file_input_targeted_directly_reads_as_before() {
    assert_eq!(
        summary(&outcome(FileInputVia::Itself, false), "\"File\""),
        "set files on \"File\" (page reports: a.txt, b.txt)"
    );
}

#[test]
fn a_css_target_that_matched_nothing_says_targets_are_refs_or_names() {
    let missing = AppError::new(ErrorCode::ElementNotFound, "no control matches");
    let explained = css_explained(missing, "input[type=file]");
    assert_eq!(explained.code, ErrorCode::ElementNotFound);
    assert!(
        explained
            .message
            .contains("prefix an explicit file input selector with css=")
    );
    let named = css_explained(
        AppError::new(ErrorCode::ElementNotFound, "no control matches"),
        "Attach",
    );
    assert_eq!(named.message, "no control matches");
}
