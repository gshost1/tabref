use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::AppError;

use super::do_expand::expand;
use super::do_step::run_step;
use super::page_view::current_page_view;
use super::step_words::split_steps;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Runs the steps of `text`, or of stdin when `text` is absent or `-`, in
/// order, each within `step_timeout_ms` and all within `--timeout`. The
/// result is one `ok` line per step, the last step's with what it set off.
/// The first step that fails stops the batch: the error names the step and
/// what was wrong, and its detail lists the steps that ran and a view of the
/// page at that point.
pub(crate) fn run(
    session: &mut Session,
    text: Option<&str>,
    step_timeout_ms: u64,
) -> Result<Outcome, AppError> {
    let source = match text {
        Some(text) if text != "-" => text.to_owned(),
        _ => session.command_stdin()?,
    };
    let steps = expand(&split_steps(&source)?)?;
    if steps.is_empty() {
        return Err(AppError::invalid_args(
            "no steps: pass them as one quoted argument or on stdin (tabref do <<'EOF' ... EOF)",
        ));
    }
    let mut lines: Vec<String> = Vec::new();
    let mut done: Vec<Value> = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let last = index + 1 == steps.len();
        let deadline = session
            .deadline()
            .capped(Duration::from_millis(step_timeout_ms));
        match run_step(session, step, last, deadline) {
            Ok(finished) => {
                done.push(
                    json!({ "step": step.source, "text": finished.text, "data": finished.data }),
                );
                lines.push(finished.text);
            }
            Err(error) => {
                let ran = Ran {
                    lines: &lines,
                    data: done,
                };
                return Err(failed(
                    session,
                    error,
                    (index, steps.len()),
                    &step.source,
                    ran,
                ));
            }
        }
    }
    Ok(Outcome::new(
        lines.join("\n"),
        json!({ "steps": done, "count": steps.len() }),
    ))
}

struct Ran<'a> {
    lines: &'a [String],
    data: Vec<Value>,
}

fn failed(
    session: &mut Session,
    mut error: AppError,
    (index, total): (usize, usize),
    source: &str,
    ran: Ran<'_>,
) -> AppError {
    let skipped = total - index - 1;
    let mut message = format!("step {} `{source}`: {}", index + 1, error.message);
    if let Some(hint) = error.suggestion.take() {
        message.push_str(&format!("; {hint}"));
    }
    if skipped > 0 {
        message.push_str(&format!("; the {skipped} step(s) after it did not run"));
    }
    error.message = message;
    let mut detail: Vec<String> = ran.lines.to_vec();
    if let Some(cdp) = error.platform_detail.take() {
        detail.push(cdp);
    }
    if let Ok(report) = current_page_view(session) {
        detail.push(report.text);
    }
    if !detail.is_empty() {
        error.platform_detail = Some(detail.join("\n"));
    }
    let mut details = error.details.take().unwrap_or_else(|| json!({}));
    if let Some(object) = details.as_object_mut() {
        object.insert("failed_step".into(), json!(index + 1));
        object.insert("steps_done".into(), json!(ran.data));
    }
    error.details = Some(details);
    error
}
