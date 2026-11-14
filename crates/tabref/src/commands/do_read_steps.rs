//! The `do` steps that move to a page, check it or read it out, rather than
//! act on one element.

use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::navigate;
use tabref_core::{AppError, Deadline};
use tabref_page::{LocateIntent, PageInfo, Word, evaluate, page_text, read_element};

use super::act_target::resolve_target;
use super::batch_step::BatchStep;
use super::open::normalize_url;
use super::page_url::{resolve_url, short_url};
use super::step_done::StepDone;
use super::target_spec::TargetSpec;
use super::wait::wait_for;
use super::wait_condition::Condition;
use crate::output::page_report::page_report;
use crate::session::Session;
use crate::view::find_matches::find_matches;
use crate::view::node_line::quote_text;
use crate::view::view_mode::ViewMode;

const READ_LIMIT: usize = 4_000;
const FOUND_SHOWN: usize = 10;

/// `open URL`: a path, `?query` or `./relative` address resolves against
/// the current page. Waits for the page's load; a page still loading after
/// three quarters of the step's time is read as it stands.
pub(crate) fn open_step(
    session: &mut Session,
    args: &[Word],
    last: bool,
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    let [address] = args else {
        return Err(AppError::invalid_args(
            "open takes one address, as in: open /admin/",
        ));
    };
    let page = session.page()?;
    let current = PageInfo::read(session.connection(), &page.target_id, deadline)?.url;
    let url = normalize_url(&resolve_url(&current, &address.text));
    let navigation = deadline.capped(Duration::from_millis(deadline.remaining_ms() * 3 / 4));
    match navigate(session.connection(), &page, &url, navigation) {
        Ok(()) => {}
        Err(error) if error.kind() == Some("cdp_load_timeout") => {}
        Err(error) => return Err(error),
    }
    let info = PageInfo::read(session.connection(), &page.target_id, deadline)?;
    let mut text = format!(
        "ok open {} {}",
        short_url(&current, &info.url),
        quote_text(&info.title)
    );
    let mut data = json!({ "action": "open", "url": info.url, "title": info.title });
    if last {
        let report = page_report(&session.capture()?, ViewMode::Interactive);
        text.push('\n');
        text.push_str(&report.text);
        data["page"] = report.data;
    }
    Ok(StepDone { text, data })
}

/// `expect text "Saved"`, `expect gone "Loading"`, `expect url ~saved=1`,
/// `expect idle`; `wait` is the same step. Fails the batch when the step's
/// time runs out first.
pub(crate) fn expect_step(
    session: &mut Session,
    step: &BatchStep,
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    let condition = Condition::from_words(step.args())?;
    let detail = wait_for(session, &condition, deadline, false)?;
    let mut text = format!("ok {}", step.source);
    if let Condition::Url(_) = condition {
        text.push_str(&format!(" ({})", short_url(&detail, &detail)));
    }
    Ok(StepDone {
        text,
        data: json!({ "action": "expect", "condition": condition.describe() }),
    })
}

/// `read` prints the page's text, cut at 4,000 characters; `read TARGET`
/// prints one element's value or text.
pub(crate) fn read_step(
    session: &mut Session,
    args: &[Word],
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    if args.is_empty() {
        let page = session.page()?;
        let (text, cut) = page_text(session.connection(), &page, READ_LIMIT, deadline)?;
        let mut shown = format!("ok read\n{}", text.trim());
        if cut {
            shown.push_str("\n... cut; read a target or find text for the rest");
        }
        return Ok(StepDone {
            text: shown,
            data: json!({ "action": "read", "text": text, "truncated": cut }),
        });
    }
    let spec = TargetSpec::from_words(args)?;
    let target = resolve_target(session, &spec, LocateIntent::Read, deadline)?;
    let reading = read_element(
        session.connection(),
        &target.session,
        &target.token,
        deadline,
    )?;
    let shown = match (&reading.value, &reading.text) {
        (Some(value), _) => format!("= {}", quote_text(value)),
        (None, Some(text)) => quote_text(text.trim()),
        (None, None) => "(empty)".to_owned(),
    };
    let mut text = format!("ok read {} {shown}", target.label);
    if let Some(checked) = reading.checked {
        text.push_str(if checked { " [checked]" } else { " [unchecked]" });
    }
    Ok(StepDone {
        text,
        data: json!({
            "action": "read",
            "target": target.label,
            "value": reading.value,
            "text": reading.text,
            "checked": reading.checked,
        }),
    })
}

/// `find TEXT`: the page's lines that hold it, with the ref to act on.
pub(crate) fn find_step(session: &mut Session, args: &[Word]) -> Result<StepDone, AppError> {
    let needle = joined(args);
    if needle.is_empty() {
        return Err(AppError::invalid_args("find needs the text to look for"));
    }
    let captured = session.capture()?;
    let matches = find_matches(&captured.tree, &needle, None);
    let mut text = format!("ok find {} ({} matches)", quote_text(&needle), matches.len());
    for line in matches.iter().take(FOUND_SHOWN) {
        text.push_str("\n  ");
        text.push_str(line);
    }
    if matches.len() > FOUND_SHOWN {
        text.push_str(&format!("\n  +{} more", matches.len() - FOUND_SHOWN));
    }
    Ok(StepDone {
        text,
        data: json!({ "action": "find", "matches": matches }),
    })
}

/// `js "EXPRESSION"`: the expression's value as compact JSON.
pub(crate) fn js_step(
    session: &mut Session,
    args: &[Word],
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    let expression = joined(args);
    let page = session.page()?;
    let value = evaluate(session.connection(), &page, &expression, deadline)?;
    let shown = match &value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    Ok(StepDone {
        text: format!("ok js -> {shown}"),
        data: json!({ "action": "js", "value": value }),
    })
}

/// `snap`: a snapshot of the page, as `tabref snap` prints it.
pub(crate) fn snap_step(session: &mut Session) -> Result<StepDone, AppError> {
    let report = page_report(&session.capture()?, ViewMode::Interactive);
    Ok(StepDone {
        text: format!("ok snap\n{}", report.text),
        data: report.data,
    })
}

fn joined(args: &[Word]) -> String {
    args.iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}
