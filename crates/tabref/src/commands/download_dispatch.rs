use std::cell::RefCell;

use tabref_core::AppError;
use tabref_page::DownloadRecord;

use super::reconcile;
use crate::commands::download_ledger::DownloadLedger;
use crate::output::outcome::Outcome;
use crate::session::Session;

thread_local! { static ERRORS: RefCell<Vec<AppError>> = const { RefCell::new(Vec::new()) }; }

pub(crate) fn note_error(error: AppError) {
    ERRORS.with_borrow_mut(|errors| {
        if !errors.contains(&error) {
            errors.push(error);
        }
    });
}

fn flush(outcome: Result<Outcome, AppError>) -> Result<Outcome, AppError> {
    ERRORS
        .with(|errors| std::mem::take(&mut *errors.borrow_mut()))
        .into_iter()
        .fold(outcome, |outcome, error| finish(outcome, Err(error)))
}

/// Reconciles only watched leftovers in an in-process command. The usual
/// path reads the bounded local ledger without a lock or a CDP request.
pub(crate) fn after_command(
    session: &mut Session,
    outcome: Result<Outcome, AppError>,
) -> Result<Outcome, AppError> {
    if crate::daemon::is_daemon_process() || !DownloadLedger::of(session).pending() {
        return flush(outcome);
    }
    let deadline = session.deadline();
    flush(finish(outcome, reconcile(session, deadline)))
}

fn finish(
    outcome: Result<Outcome, AppError>,
    reconciled: Result<Vec<DownloadRecord>, AppError>,
) -> Result<Outcome, AppError> {
    match reconciled {
        Ok(records) => report(outcome, records),
        Err(error) => {
            let text = format!("note: download reconciliation failed: {}", error.message);
            let note = serde_json::json!({"message": error.message, "details": error.details});
            match outcome {
                Ok(mut outcome) => {
                    outcome.push_line(&text);
                    if let Some(data) = outcome.data.as_object_mut() {
                        append_note(data, note);
                    }
                    Ok(outcome)
                }
                Err(mut original) => {
                    original.message.push_str(&format!("\n{text}"));
                    let details = original
                        .details
                        .get_or_insert_with(|| serde_json::json!({}));
                    if let Some(details) = details.as_object_mut() {
                        append_note(details, note);
                    }
                    Err(original)
                }
            }
        }
    }
}

fn append_note(data: &mut serde_json::Map<String, serde_json::Value>, note: serde_json::Value) {
    if let Some(existing) = data
        .get_mut("download_reconciliation_error")
        .and_then(serde_json::Value::as_object_mut)
    {
        let additional = existing
            .entry("additional_errors")
            .or_insert_with(|| serde_json::json!([]));
        if let Some(additional) = additional.as_array_mut() {
            additional.push(note);
        }
    } else {
        data.insert("download_reconciliation_error".into(), note);
    }
}

fn report(
    outcome: Result<Outcome, AppError>,
    records: Vec<DownloadRecord>,
) -> Result<Outcome, AppError> {
    if records.is_empty() {
        return outcome;
    }
    match outcome {
        Ok(mut outcome) => {
            outcome.extend(records);
            Ok(outcome)
        }
        Err(mut error) => {
            let mut report = Outcome::new(
                error.message,
                error
                    .details
                    .take()
                    .unwrap_or_else(|| serde_json::json!({})),
            );
            report.extend(records);
            error.message = report.text;
            error.details = Some(report.data);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tabref_page::{DownloadOrigin, DownloadState, DownloadTiming};

    use super::*;

    #[test]
    fn queued_write_failures_are_reported_once_without_losing_other_notes() {
        let first = AppError::invalid_args("tracking full").with_details(json!({"kind":"full"}));
        note_error(first.clone());
        note_error(first);
        note_error(
            AppError::invalid_args("quarantined").with_details(json!({"backup":"saved.json"})),
        );
        let result = flush(Ok(Outcome::new("paid", json!({"paid":true})))).unwrap();
        assert_eq!(result.data["paid"], true);
        assert_eq!(
            result.data["download_reconciliation_error"]["details"]["kind"],
            "full"
        );
        assert_eq!(
            result.data["download_reconciliation_error"]["additional_errors"][0]["details"]["backup"],
            "saved.json"
        );
        assert_eq!(result.text.matches("tracking full").count(), 1);
        let next = flush(Ok(Outcome::new("1", json!({"value":1})))).unwrap();
        assert!(next.data["download_reconciliation_error"].is_null());
        let original = AppError::timeout("original");
        note_error(AppError::invalid_args("tracking full"));
        let error = flush(Err(original.clone())).unwrap_err();
        assert_eq!(error.code, original.code);
        assert_eq!(error.disposition, original.disposition);
        assert!(error.message.starts_with("original\nnote:"));
    }

    fn capped() -> DownloadRecord {
        DownloadRecord {
            guid: "g".into(),
            origin: DownloadOrigin {
                suggested_name: "slow.bin".into(),
                ..DownloadOrigin::default()
            },
            state: DownloadState::TooLarge { cap_mb: 1 },
            bytes: 2_097_152,
            expected: 0,
            path: None,
            timing: DownloadTiming::default(),
        }
    }

    #[test]
    fn empty_reports_leave_successes_and_errors_unchanged() {
        let outcome = Ok(Outcome::new("1", json!({"value": 1})));
        assert_eq!(report(outcome.clone(), vec![]), outcome);
        let error = Err(AppError::invalid_args("bad input"));
        assert_eq!(report(error.clone(), vec![]), error);
    }

    #[test]
    fn success_keeps_its_value_and_adds_the_existing_download_shape() {
        let outcome = report(Ok(Outcome::new("1", json!({"value": 1}))), vec![capped()]).unwrap();
        assert_eq!(outcome.data["value"], 1);
        assert_eq!(
            outcome.data["downloads"][0],
            crate::commands::download_lines::download_json(&capped())
        );
        assert!(outcome.text.contains("over the 1 MB download cap"));
    }

    #[test]
    fn error_keeps_its_code_disposition_and_existing_details() {
        let before = AppError::invalid_args("bad input").with_details(json!({"kind": "old"}));
        let error = report(Err(before.clone()), vec![capped()]).unwrap_err();
        assert_eq!(error.code, before.code);
        assert_eq!(error.disposition, before.disposition);
        assert_eq!(error.kind(), Some("old"));
        assert_eq!(
            error.details.as_ref().unwrap()["downloads"][0]["name"],
            "slow.bin"
        );
        assert!(error.message.contains("over the 1 MB download cap"));
    }

    #[test]
    fn a_ledger_io_failure_never_changes_command_success_or_delivery() {
        let failure: AppError = std::fs::File::open(
            std::env::temp_dir().join(format!("missing-ledger-{}", std::process::id())),
        )
        .unwrap_err()
        .into();
        let result = finish(
            Ok(Outcome::new("paid", json!({"paid": true}))),
            Err(failure.clone()),
        )
        .unwrap();
        assert_eq!(result.data["paid"], true);
        assert!(
            result
                .text
                .starts_with("paid\nnote: download reconciliation failed:")
        );
        assert_eq!(
            result.data["download_reconciliation_error"]["details"]["kind"],
            "io"
        );
        let before = AppError::timeout("original").with_details(json!({"kind":"original"}));
        let result = finish(Err(before.clone()), Err(failure)).unwrap_err();
        assert_eq!(result.code, before.code);
        assert_eq!(result.disposition, before.disposition);
        assert_eq!(result.suggestion, before.suggestion);
        assert_eq!(result.kind(), Some("original"));
        assert!(result.message.starts_with("original\nnote:"));
        assert!(result.details.unwrap()["download_reconciliation_error"].is_object());
    }
}
