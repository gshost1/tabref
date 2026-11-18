use std::path::PathBuf;

use serde_json::json;
use tabref_core::{AppError, ErrorCode};
use tabref_page::{FileInputVia, LocateIntent, UploadOutcome, upload_files};

use super::act_target::resolve_target;
use super::target_spec::{TargetSpec, looks_like_css};
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::escape::escape_text;

/// Sets the files of a file input without opening a file dialog. The
/// target may be the drop zone, link or label that stands for a hidden
/// input, or, on a page with no file input, the button whose click opens
/// the file chooser; the result names the input that took the files.
pub(crate) fn run(
    session: &mut Session,
    target: &str,
    files: &[PathBuf],
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let explain = |error: AppError| css_explained(error, target);
    let spec = TargetSpec::from_arg(target).map_err(explain)?;
    let target = resolve_target(session, &spec, LocateIntent::Click, deadline).map_err(explain)?;
    let uploaded = upload_files(
        session.connection(),
        &target.session,
        &target.token,
        files,
        deadline,
    )?;
    let text = summary(&uploaded, &target.label);
    Ok(Outcome::new(
        text,
        json!({
            "action": "upload",
            "target": target.label,
            "input": uploaded.input,
            "input_hidden": uploaded.hidden,
            "files": uploaded.files,
            "verified": uploaded.verified,
        }),
    ))
}

fn summary(uploaded: &UploadOutcome, label: &str) -> String {
    let names: Vec<String> = uploaded.files.iter().map(|name| printable(name)).collect();
    let mut text = format!("uploaded {} to {label}", names.join(", "));
    if uploaded.via != FileInputVia::Itself {
        let hidden = if uploaded.hidden { "hidden " } else { "" };
        text.push_str(&format!(
            " via {hidden}{} ({})",
            uploaded.input,
            uploaded.via.describe()
        ));
    }
    if !uploaded.verified {
        text.push_str(" (the input does not show them)");
    }
    text
}

/// A file name as the page's input reports it, fit for a result line: the
/// page can make an input report any names, so brackets are escaped and
/// line breaks and control characters dropped, and it cannot forge a ref
/// or a second line.
fn printable(name: &str) -> String {
    escape_text(name)
        .chars()
        .filter(|character| !character.is_control())
        .collect()
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
            "{target:?} reads as a CSS selector, and targets are refs or visible names, not CSS"
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
