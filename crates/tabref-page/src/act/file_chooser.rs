use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::CdpEvent;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::click::click_node;
use super::element_call::ElementCall;
use super::file_input_choice::{FileInputVia, safe_name};
use super::remote_element::call_function;
use super::upload::shows_files;
use super::upload_outcome::UploadOutcome;
use crate::{CdpConnection, ElementToken, PageSession};

const CHOOSER_OPENED: &str = "Page.fileChooserOpened";
const INTERCEPT: &str = "Page.setInterceptFileChooserDialog";
const LEFT_ON: &str = "tabref could not turn file-chooser interception off again in this tab";
const UNDO: &str = "Run 'tabref daemon stop': until tabref lets go of the tab, an upload button \
                    a person clicks there opens no file dialog";
/// How long a click may take to open its file chooser: a page that builds
/// its input in a handler does so in the same task as the click.
const CHOOSER_WAIT: Duration = Duration::from_millis(2_000);
/// The time kept for turning interception off again, whatever became of
/// the command's own deadline.
const RESTORE_MS: u64 = 1_500;

const READ_INPUT: &str = r"function() {
  const tidy = (text) => String(text || '').slice(0, 100).replace(/[^A-Za-z0-9_.-]/g, '');
  let name = 'input[type=file]';
  if (tidy(this.id)) name = 'input#' + tidy(this.id);
  else if (tidy(this.name)) name = 'input[name=' + tidy(this.name) + ']';
  return { name, files: Array.from(this.files || []).map((file) => file.name) };
}";

/// Uploads through the file chooser a click on `token` opens, for a page
/// that holds no file input until its upload button is pressed (the button
/// builds one, often never attached to the document, and clicks it). The
/// browser is told to hand the chooser to tabref instead of showing the
/// native dialog, the target is clicked, and the files are set on the
/// input the chooser belongs to. Interception is turned off again before
/// returning, whatever happened: on a deadline of its own, and even for a
/// caller that was cancelled, since left on it would swallow the dialog of
/// every later click a person makes in the tab. When it cannot be turned
/// off the result is an error that says so, files set or not. The click is
/// real, so every failure after it reports the click as delivered.
pub(crate) fn upload_through_chooser(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    paths: &[String],
    deadline: Deadline,
) -> Result<UploadOutcome, AppError> {
    let id = Some(session.session_id.as_str());
    connection.call("Page.enable", json!({}), id, deadline)?;
    let armed = connection.call(INTERCEPT, json!({ "enabled": true }), id, deadline);
    connection.discard_events(|event| is_chooser(event, session));
    let outcome = armed.and_then(|_| click_and_set(connection, session, token, paths, deadline));
    let restore = Deadline::after(RESTORE_MS).unwrap_or(deadline);
    let restored = connection.call_restoring(INTERCEPT, json!({ "enabled": false }), id, restore);
    connection.discard_events(|event| is_chooser(event, session));
    match (outcome, restored) {
        (outcome, Ok(_)) => outcome,
        (Ok(uploaded), Err(error)) => Err(left_intercepting(&uploaded, &error)),
        (Err(error), Err(_)) => Err(AppError {
            message: format!("{}; {LEFT_ON}", error.message),
            suggestion: Some(UNDO.to_owned()),
            ..error
        }),
    }
}

fn is_chooser(event: &CdpEvent, session: &PageSession) -> bool {
    event.method == CHOOSER_OPENED
        && event.session_id.as_deref() == Some(session.session_id.as_str())
}

fn click_and_set(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    paths: &[String],
    deadline: Deadline,
) -> Result<UploadOutcome, AppError> {
    click_node(connection, session, token, deadline)?;
    set_on_chooser(connection, session, token, paths, deadline)
        .map_err(|error| error.with_disposition(DeliverySemantics::delivered_unverified()))
}

/// Waits for the chooser the click opened and sets the files on its input.
/// A wait that ends because the command's own deadline ran out is that
/// timeout, not a page that opened no chooser.
fn set_on_chooser(
    connection: &mut CdpConnection,
    session: &PageSession,
    token: &ElementToken,
    paths: &[String],
    deadline: Deadline,
) -> Result<UploadOutcome, AppError> {
    let opened = connection
        .wait_for_event(
            |event| is_chooser(event, session),
            deadline.capped(CHOOSER_WAIT),
        )
        .map_err(|error| match error.code {
            ErrorCode::Timeout if !deadline.is_expired() => no_chooser(token.backend_node_id),
            _ => error,
        })?;
    let input = opened
        .params
        .get("backendNodeId")
        .and_then(Value::as_i64)
        .ok_or_else(|| no_chooser_input(token.backend_node_id))?;
    let multiple = opened.params.get("mode").and_then(Value::as_str) == Some("selectMultiple");
    if paths.len() > 1 && !multiple {
        return Err(AppError::invalid_args(format!(
            "The file chooser DOM node {} opened takes one file, not {}; it was clicked and no files were sent",
            token.backend_node_id,
            paths.len()
        )));
    }
    connection
        .call(
            "DOM.setFileInputFiles",
            json!({ "files": paths, "backendNodeId": input }),
            Some(&session.session_id),
            deadline,
        )
        .map_err(|error| AppError {
            message: format!(
                "Setting files through the file chooser failed: {}",
                error.message
            ),
            ..error
        })?;
    let (name, files) = read_input(connection, session, input, deadline).unwrap_or_default();
    Ok(UploadOutcome {
        verified: shows_files(paths, &files),
        files,
        input: name,
        hidden: false,
        via: FileInputVia::Chooser,
    })
}

/// The chooser's input as the result names it, and the file names it
/// holds. An input that cannot be read back leaves both empty, which the
/// result reports as files the input does not show.
fn read_input(
    connection: &mut CdpConnection,
    session: &PageSession,
    backend_node_id: i64,
    deadline: Deadline,
) -> Option<(String, Vec<String>)> {
    let id = Some(session.session_id.as_str());
    let resolved = connection
        .call(
            "DOM.resolveNode",
            json!({ "backendNodeId": backend_node_id }),
            id,
            deadline,
        )
        .ok()?;
    let object = resolved.pointer("/object/objectId")?.as_str()?.to_owned();
    let read = call_function(
        connection,
        session,
        &object,
        ElementCall {
            function: READ_INPUT,
            arguments: Vec::new(),
        },
        deadline,
    );
    let _ = connection.call(
        "Runtime.releaseObject",
        json!({ "objectId": object }),
        id,
        deadline,
    );
    let read = read.ok()?;
    let name = safe_name(read.get("name").and_then(Value::as_str).unwrap_or_default());
    let files = read
        .get("files")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    Some((name, files))
}

fn left_intercepting(uploaded: &UploadOutcome, cause: &AppError) -> AppError {
    let delivery = if uploaded.verified {
        DeliverySemantics::delivered_verified()
    } else {
        DeliverySemantics::delivered_unverified()
    };
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "{} files were set on {}, but {LEFT_ON}: {}",
            uploaded.files.len(),
            uploaded.input,
            cause.message
        ),
    )
    .with_suggestion(UNDO)
    .with_details(json!({ "kind": "file_chooser_left_intercepted" }))
    .with_disposition(delivery)
}

fn no_chooser(backend_node_id: i64) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!(
            "DOM node {backend_node_id} is not a file input, there is no <input type=file> on the page, hidden ones included, and clicking it opened no file chooser"
        ),
    )
    .with_suggestion(
        "The click was delivered. If it opened a menu, upload to the menu item that picks a file",
    )
    .with_details(json!({ "kind": "no_file_chooser" }))
}

fn no_chooser_input(backend_node_id: i64) -> AppError {
    AppError::new(
        ErrorCode::ActionNotSupported,
        format!(
            "Clicking DOM node {backend_node_id} opened a file chooser that names no input to take the files"
        ),
    )
    .with_details(json!({ "kind": "no_file_chooser" }))
}

#[cfg(test)]
#[path = "file_chooser_tests.rs"]
mod tests;
