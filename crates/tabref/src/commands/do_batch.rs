use std::io::{Read, Write};
use std::time::Duration;

use serde_json::{Value, json};
use tabref_core::AppError;

use super::do_expand::expand;
use super::do_step::run_step;
use super::step_words::split_steps;
use crate::output::outcome::Outcome;
use crate::output::page_report::page_report;
use crate::session::Session;
use crate::view::view_mode::ViewMode;

/// How the batch runs: each step's time limit, and whether results print
/// as text while the steps run or as one JSON envelope at the end.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BatchMode {
    pub(crate) step_timeout_ms: u64,
    pub(crate) json: bool,
}

/// Runs the steps of `text`, or of stdin when `text` is absent or `-`, in
/// order, each within its own time limit and all within `--timeout`. In
/// text mode each step's line prints as it finishes. The first step that
/// fails stops the batch: the error names the step and what to do, and its
/// detail is a snapshot of the page at that point.
pub(crate) fn run(
    session: &mut Session,
    text: Option<&str>,
    mode: BatchMode,
) -> Result<Outcome, AppError> {
    let source = match text {
        Some(text) if text != "-" => text.to_owned(),
        _ => {
            let mut read = String::new();
            std::io::stdin().read_to_string(&mut read)?;
            read
        }
    };
    let steps = expand(&split_steps(&source)?)?;
    if steps.is_empty() {
        return Err(AppError::invalid_args(
            "no steps: pass them as one quoted argument or on stdin (tabref do <<'EOF' ... EOF)",
        ));
    }
    let mut done: Vec<Value> = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let last = index + 1 == steps.len();
        let deadline = session
            .deadline()
            .capped(Duration::from_millis(mode.step_timeout_ms));
        match run_step(session, step, last, deadline) {
            Ok(finished) => {
                if !mode.json {
                    let mut stdout = std::io::stdout().lock();
                    let _ = writeln!(stdout, "{}", finished.text);
                    let _ = stdout.flush();
                }
                done.push(json!({ "step": step.source, "text": finished.text, "data": finished.data }));
            }
            Err(error) => {
                return Err(failed(session, error, (index, steps.len()), &step.source, done));
            }
        }
    }
    Ok(Outcome::new(
        String::new(),
        json!({ "steps": done, "count": steps.len() }),
    ))
}

fn failed(
    session: &mut Session,
    mut error: AppError,
    (index, total): (usize, usize),
    source: &str,
    done: Vec<Value>,
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
    if let Ok(captured) = session.capture() {
        let report = page_report(&captured, ViewMode::Interactive);
        let detail = match error.platform_detail.take() {
            Some(cdp) => format!("{cdp}\n{}", report.text),
            None => report.text,
        };
        error.platform_detail = Some(detail);
    }
    let mut details = error.details.take().unwrap_or_else(|| json!({}));
    if let Some(object) = details.as_object_mut() {
        object.insert("failed_step".into(), json!(index + 1));
        object.insert("steps_done".into(), json!(done));
    }
    error.details = Some(details);
    error
}
