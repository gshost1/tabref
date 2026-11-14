use std::path::PathBuf;

use serde_json::json;
use tabref_core::AppError;
use tabref_page::{LocateIntent, upload_files};

use super::act_target::resolve_target;
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Sets the files of a file input without opening a file dialog.
pub(crate) fn run(
    session: &mut Session,
    target: &str,
    files: &[PathBuf],
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let spec = TargetSpec::from_arg(target)?;
    let target = resolve_target(session, &spec, LocateIntent::Click, deadline)?;
    let uploaded = upload_files(
        session.connection(),
        &target.session,
        &target.token,
        files,
        deadline,
    )?;
    let mut text = format!("uploaded {} to {}", uploaded.files.join(", "), target.label);
    if !uploaded.verified {
        text.push_str(" (the input does not show them)");
    }
    Ok(Outcome::new(
        text,
        json!({
            "action": "upload",
            "target": target.label,
            "files": uploaded.files,
            "verified": uploaded.verified,
        }),
    ))
}
