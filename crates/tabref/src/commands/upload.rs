use std::path::PathBuf;

use serde_json::json;
use tabref_core::{AppError, ErrorCode};
use tabref_page::{FileInputVia, LocateIntent, UploadOutcome, upload_files, upload_input_files};

use super::act_target::resolve_target;
use super::target_spec::{TargetSpec, looks_like_css};
#[path = "upload_css.rs"]
mod css;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Sets the files of a file input without opening a file dialog. The
/// target may be the drop zone, link or label that stands for a hidden
/// input; the result names the input that took the files.
pub(crate) fn run(
    session: &mut Session,
    target: &str,
    files: &[PathBuf],
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let (uploaded, label) = if let Some(selector) = target.trim().strip_prefix("css=") {
        let (page, token) = css::resolve(session, selector, deadline)?;
        let uploaded = upload_input_files(session.connection(), &page, &token, files, deadline)?;
        (uploaded, format!("{target:?}"))
    } else {
        let explain = |error: AppError| css_explained(error, target);
        let spec = TargetSpec::from_arg(target).map_err(explain)?;
        let target =
            resolve_target(session, &spec, LocateIntent::Click, deadline).map_err(explain)?;
        let uploaded = upload_files(
            session.connection(),
            &target.session,
            &target.token,
            files,
            deadline,
        )?;
        (uploaded, target.label)
    };
    let text = summary(&uploaded, &label);
    Ok(Outcome::new(
        text,
        json!({
            "action": "upload",
            "target": label,
            "input": uploaded.input,
            "input_hidden": uploaded.hidden,
            "via": uploaded.via.as_str(),
            "files": uploaded.files,
            "page_reports_requested_files": uploaded.page_reports_requested_files,
        }),
    ))
}

fn summary(uploaded: &UploadOutcome, label: &str) -> String {
    let mut text = format!("set files on {label}");
    if uploaded.via != FileInputVia::Itself {
        let hidden = if uploaded.hidden { "hidden " } else { "" };
        text.push_str(&format!(
            " via {hidden}{} ({})",
            uploaded.input,
            uploaded.via.describe()
        ));
    }
    if uploaded.files.is_empty() {
        text.push_str(" (page reports no files)");
    } else {
        text.push_str(&format!(" (page reports: {})", uploaded.files.join(", ")));
    }
    if !uploaded.page_reports_requested_files {
        text.push_str(" (page report differs from request)");
    }
    text
}

fn css_explained(error: AppError, target: &str) -> AppError {
    let unmatched = matches!(
        error.code,
        ErrorCode::ElementNotFound | ErrorCode::InvalidArgs
    );
    if !unmatched || !looks_like_css(target) {
        return error;
    }
    AppError {
        message: format!(
            "{target:?} reads as a CSS selector. Use a ref or visible name, or prefix an explicit file input selector with css="
        ),
        suggestion: Some(
            "Name the visible control (a drop zone's link or button works: upload finds the file input behind it), or use a ref from 'tabref snap'"
                .to_owned(),
        ),
        ..error
    }
}

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
