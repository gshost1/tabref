use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::element_call::ElementCall;
use super::element_errors::node_error;
use super::remote_element::{call_function, with_element};
use super::upload_outcome::UploadOutcome;
use crate::{CdpConnection, ElementToken, PageSession};

const PREPARE_UPLOAD: &str = r"function() {
  if (this.localName !== 'input' || this.type !== 'file') return { error: 'not_file_input', tag: this.localName, type: this.type || null };
  if (this.disabled) return { error: 'disabled' };
  return { multiple: this.multiple };
}";

const READ_FILES: &str = r"function() {
  return Array.from(this.files || []).map((file) => file.name);
}";

/// Sets the files of an `<input type=file>` with `DOM.setFileInputFiles`,
/// which fires the input's `input` and `change` events as a user's pick
/// would, then reads back the names it holds. No native file dialog opens.
/// A path that is not an existing file, several files for a single-file
/// input, or a node that is not a file input is refused before anything is
/// set.
pub fn upload_files(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    files: &[PathBuf],
    deadline: Deadline,
) -> Result<UploadOutcome, AppError> {
    let paths = absolute_files(files)?;
    with_element(
        connection,
        session,
        token,
        deadline,
        |connection, object| {
            let prepared = call_function(
                connection,
                session,
                object,
                script(PREPARE_UPLOAD),
                deadline,
            )?;
            if let Some(reason) = prepared.get("error").and_then(Value::as_str) {
                return Err(not_file_input(token.backend_node_id, reason));
            }
            let multiple = prepared.get("multiple").and_then(Value::as_bool) == Some(true);
            if paths.len() > 1 && !multiple {
                return Err(AppError::invalid_args(format!(
                    "DOM node {} takes one file, not {}",
                    token.backend_node_id,
                    paths.len()
                )));
            }
            connection
                .call(
                    "DOM.setFileInputFiles",
                    json!({ "files": paths, "backendNodeId": token.backend_node_id }),
                    Some(&session.session_id),
                    deadline,
                )
                .map_err(|error| node_error(error, token))?;
            let read = call_function(connection, session, object, script(READ_FILES), deadline)?;
            let names: Vec<String> = read
                .as_array()
                .map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let verified = names.len() == paths.len()
                && paths.iter().zip(&names).all(|(path, name)| {
                    Path::new(path)
                        .file_name()
                        .is_some_and(|file| file.to_string_lossy() == name.as_str())
                });
            Ok(UploadOutcome {
                files: names,
                verified,
            })
        },
    )
}

fn absolute_files(files: &[PathBuf]) -> Result<Vec<String>, AppError> {
    if files.is_empty() {
        return Err(AppError::invalid_args("Name at least one file to upload"));
    }
    files
        .iter()
        .map(|file| {
            let absolute = std::path::absolute(file).map_err(|error| {
                AppError::invalid_args(format!("Cannot upload '{}': {error}", file.display()))
            })?;
            if !absolute.is_file() {
                return Err(AppError::invalid_args(format!(
                    "Cannot upload '{}': not a file",
                    file.display()
                )));
            }
            Ok(absolute.to_string_lossy().into_owned())
        })
        .collect()
}

fn script(function: &'static str) -> ElementCall {
    ElementCall {
        function,
        arguments: Vec::new(),
    }
}

fn not_file_input(backend_node_id: i64, reason: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!("DOM node {backend_node_id} is not an enabled <input type=file>"),
    )
    .with_suggestion("Upload to the file input's own ref; find it with 'tabref snap -a'")
    .with_details(json!({ "kind": "cdp_not_file_input", "reason": reason }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
