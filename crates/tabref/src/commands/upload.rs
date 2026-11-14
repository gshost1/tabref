use std::path::PathBuf;

use serde_json::json;
use tabref_core::{AppError, DeliverySemantics};
use tabref_page::upload_files;

use crate::output::outcome::Outcome;
use crate::session::Session;

/// Sets the files of a file input without opening a file dialog.
pub(crate) fn run(
    session: &mut Session,
    target: &str,
    files: &[PathBuf],
) -> Result<Outcome, AppError> {
    let target = session.resolve_ref(target)?;
    let deadline = session.deadline();
    let uploaded = upload_files(
        session.connection(),
        &target.session,
        &target.token,
        files,
        deadline,
    )?;
    let disposition = if uploaded.verified {
        DeliverySemantics::delivered_verified()
    } else {
        DeliverySemantics::delivered_unverified()
    };
    Ok(Outcome::new(
        format!(
            "uploaded {} to {}",
            uploaded.files.join(", "),
            target.label()
        ),
        json!({
            "action": "upload",
            "ref": target.ref_id.to_string(),
            "files": uploaded.files,
            "verified": uploaded.verified,
            "disposition": disposition,
        }),
    ))
}
