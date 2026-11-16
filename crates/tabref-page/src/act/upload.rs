use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::element_call::ElementCall;
use super::element_errors::node_error;
use super::file_input_choice::{FileInputCandidate, FileInputChoice, choose};
use super::file_input_search::{list_file_inputs, pick_file_input};
use super::remote_element::{call_function, with_element};
use super::upload_outcome::UploadOutcome;
use crate::{CdpConnection, ElementToken, PageSession};

const READ_FILES: &str = r"function() {
  return Array.from(this.files || []).map((file) => file.name);
}";

/// Sets the files of an `<input type=file>` with `DOM.setFileInputFiles`,
/// which fires the input's `input` and `change` events as a user's pick
/// would, then reads back the names it holds. No native file dialog opens.
/// A target that is not a file input stands for the one it means: the
/// input its label controls, else the nearest file input around it (a
/// drop zone's hidden input in the same form), else the only one on the
/// page. A path that is not an existing file, several files for a
/// single-file input, no file input, or several equally near are refused
/// before anything is set.
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
            let inputs = list_file_inputs(connection, session, object, deadline)?;
            let (input, via) = match choose(&inputs.candidates, inputs.top) {
                FileInputChoice::Chosen(input, via) => (input.clone(), via),
                FileInputChoice::None => return Err(no_file_input(token.backend_node_id)),
                FileInputChoice::Ambiguous(found) => {
                    return Err(ambiguous_file_input(token.backend_node_id, &found));
                }
            };
            if input.disabled {
                return Err(disabled_input(&input.name));
            }
            if paths.len() > 1 && !input.multiple {
                return Err(AppError::invalid_args(format!(
                    "{} takes one file, not {}",
                    input.name,
                    paths.len()
                ))
                .with_disposition(DeliverySemantics::not_delivered()));
            }
            let picked =
                pick_file_input(connection, session, object, input.index, token, deadline)?;
            let names = set_and_read(connection, session, &picked, &paths, token, deadline);
            let _ = connection.call(
                "Runtime.releaseObject",
                json!({ "objectId": picked }),
                Some(&session.session_id),
                deadline,
            );
            let names = names?;
            let verified = names.len() == paths.len()
                && paths.iter().zip(&names).all(|(path, name)| {
                    Path::new(path)
                        .file_name()
                        .is_some_and(|file| file.to_string_lossy() == name.as_str())
                });
            Ok(UploadOutcome {
                files: names,
                verified,
                input: input.name,
                hidden: input.hidden,
                via,
            })
        },
    )
}

fn set_and_read(
    connection: &mut CdpConnection,
    session: &PageSession,
    input: &str,
    paths: &[String],
    token: &ElementToken,
    deadline: Deadline,
) -> Result<Vec<String>, AppError> {
    connection
        .call(
            "DOM.setFileInputFiles",
            json!({ "files": paths, "objectId": input }),
            Some(&session.session_id),
            deadline,
        )
        .map_err(|error| node_error(error, token))?;
    let read = call_function(connection, session, input, script(READ_FILES), deadline)?;
    Ok(read
        .as_array()
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default())
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

fn no_file_input(backend_node_id: i64) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!(
            "DOM node {backend_node_id} is not a file input, and there is no <input type=file> on the page, hidden ones included"
        ),
    )
    .with_suggestion("Some pages add their file input only after a click: click the upload control, then upload again")
    .with_details(json!({ "kind": "no_file_input" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn ambiguous_file_input(backend_node_id: i64, found: &[&FileInputCandidate]) -> AppError {
    let names: Vec<&str> = found.iter().map(|input| input.name.as_str()).collect();
    AppError::new(
        ErrorCode::AmbiguousTarget,
        format!(
            "{} file inputs are equally near DOM node {backend_node_id}: {}",
            names.len(),
            names.join(", ")
        ),
    )
    .with_suggestion("Upload to a control inside the form or drop zone that holds the input you mean")
    .with_details(json!({ "kind": "ambiguous_file_input", "inputs": names }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn disabled_input(name: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!("{name} is disabled and takes no files"),
    )
    .with_details(json!({ "kind": "cdp_not_file_input", "reason": "disabled" }))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "upload_tests.rs"]
mod tests;
