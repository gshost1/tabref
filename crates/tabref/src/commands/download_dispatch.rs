use tabref_core::AppError;
use tabref_page::DownloadRecord;

use super::reconcile;
use crate::commands::download_ledger::DownloadLedger;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Reconciles only watched leftovers in an in-process command. The usual
/// path reads the bounded local ledger without a lock or a CDP request.
pub(crate) fn after_command(
    session: &mut Session,
    outcome: Result<Outcome, AppError>,
) -> Result<Outcome, AppError> {
    if crate::daemon::is_daemon_process() || !DownloadLedger::of(session).pending() {
        return outcome;
    }
    let deadline = session.deadline();
    finish(outcome, reconcile(session, deadline))
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
                        data.insert("download_reconciliation_error".into(), note);
                    }
                    Ok(outcome)
                }
                Err(mut original) => {
                    original.message.push_str(&format!("\n{text}"));
                    let details = original
                        .details
                        .get_or_insert_with(|| serde_json::json!({}));
                    if let Some(details) = details.as_object_mut() {
                        details.insert("download_reconciliation_error".into(), note);
                    }
                    Err(original)
                }
            }
        }
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
