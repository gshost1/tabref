use serde_json::{Value, json};
use tabref_core::AppError;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::snapshot_view::snapshot_view;
use crate::view::view_request::ViewRequest;
use crate::view::view_scope::ViewScope;

const REPORT_ERROR_FIELD: &str = "report_error";

/// A new snapshot of the current tab as `open` and `snap` print it, for an
/// action that led to another page or a batch step that failed.
pub(crate) fn current_page_view(session: &mut Session) -> Result<Outcome, AppError> {
    let captured = session.capture()?;
    let options = ViewRequest::default().snap_options(&captured.info.url)?;
    snapshot_view(session, &captured, &options, &ViewScope::Page)
}

/// The note line for `what` a delivered action's report could not read,
/// with the command that reads it. The action still counts as delivered.
pub(crate) fn unread_note(what: &str, error: &AppError) -> String {
    format!(
        "note: could not read {what} ({}); run tabref snap to see it",
        error.message
    )
}

/// Records `note` under `report_error` in the `--json` data, after any
/// earlier note there.
pub(crate) fn record_unread(data: &mut Value, note: &str) {
    let said = note.strip_prefix("note: ").unwrap_or(note);
    let Some(object) = data.as_object_mut() else {
        return;
    };
    let joined = match object.get(REPORT_ERROR_FIELD).and_then(Value::as_str) {
        Some(earlier) => format!("{earlier}; {said}"),
        None => said.to_owned(),
    };
    object.insert(REPORT_ERROR_FIELD.into(), json!(joined));
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tabref_core::AppError;

    use super::{record_unread, unread_note};

    #[test]
    fn an_unread_view_is_one_note_line_and_the_same_words_in_the_data() {
        let note = unread_note("the new tab", &AppError::invalid_args("target gone"));
        assert_eq!(
            note,
            "note: could not read the new tab (target gone); run tabref snap to see it"
        );
        let mut data = json!({ "action": "click" });
        record_unread(&mut data, &note);
        assert_eq!(
            data["report_error"],
            "could not read the new tab (target gone); run tabref snap to see it"
        );
        record_unread(
            &mut data,
            "note: could not read the page (x); run tabref snap to see it",
        );
        assert_eq!(
            data["report_error"],
            "could not read the new tab (target gone); run tabref snap to see it; could not read the page (x); run tabref snap to see it"
        );
    }
}
