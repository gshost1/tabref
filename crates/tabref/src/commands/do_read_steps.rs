//! The `do` steps that move to a page, check it or read it out, rather than
//! act on one element.

use std::time::Duration;

use serde_json::{Value, json};
use tabref_cdp::{PageSession, navigate};
use tabref_core::{AppError, Deadline};
use tabref_page::{LocateIntent, PageInfo, Word, evaluate, page_text, read_element};

use super::act_target::resolve_target;
use super::batch_step::BatchStep;
use super::downloads;
use super::new_tab::{in_new_tab, new_tab_address, open_tab};
use super::open::normalize_url;
use super::page_url::{resolve_url, short_url};
use super::page_view::current_page_view;
use super::step_done::StepDone;
use super::target_spec::TargetSpec;
use super::wait::wait_for;
use super::wait_condition::Condition;
use crate::session::Session;
use crate::view::find_matches::find_matches;
use crate::view::node_line::quote_text;

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
    let (new_tab, address) = step_address(args)?;
    let (mut text, mut data) = if new_tab {
        let url = new_tab_address(session, &address.text)?;
        let (index, page) = open_tab(session, deadline)?;
        let (mut text, mut data) = opened_line(session, &page, (&url, &url), deadline)
            .map_err(|error| in_new_tab(error, index, &url))?;
        text.push_str(&format!(" (new tab [{index}])"));
        data["new_tab_index"] = json!(index);
        (text, data)
    } else {
        let page = session.page()?;
        let current = PageInfo::read(session.connection(), &page.target_id, deadline)?.url;
        let url = normalize_url(&resolve_url(&current, &address.text));
        opened_line(session, &page, (&url, &current), deadline)?
    };
    if last {
        let report = current_page_view(session)?;
        text.push('\n');
        text.push_str(&report.text);
        data["page"] = report.data;
    }
    Ok(StepDone { text, data })
}

/// The address of an open step and whether it asks for a new tab:
/// `open URL`, `open --new-tab URL` or `open URL --new-tab`. A missing
/// address, or a bare word that starts with `--`, is refused rather than
/// read as a host name.
fn step_address(args: &[Word]) -> Result<(bool, &Word), AppError> {
    let flag = |word: &Word| !word.quoted && word.text == "--new-tab";
    let (new_tab, address) = match args {
        [first, second] if flag(first) => (true, second),
        [first, second] if flag(second) => (true, first),
        [address] => (false, address),
        _ => {
            return Err(AppError::invalid_args(
                "open takes one address, as in: open /admin/ or open --new-tab /help",
            ));
        }
    };
    if !address.quoted && address.text.starts_with("--") {
        return Err(AppError::invalid_args(format!(
            "'{}' is not an address; open takes one, as in: open --new-tab /help",
            address.text
        )));
    }
    Ok((new_tab, address))
}

/// Loads `url` in `page` and names where it led, as seen from `from`.
fn opened_line(
    session: &mut Session,
    page: &PageSession,
    (url, from): (&str, &str),
    deadline: Deadline,
) -> Result<(String, Value), AppError> {
    let navigation = deadline.capped(Duration::from_millis(deadline.remaining_ms() * 3 / 4));
    let _ = downloads::enable(session, deadline);
    let navigated = navigate(session.connection(), page, url, navigation);
    let started = downloads::after_action(session, deadline, downloads::grace_after(&navigated));
    if !started.is_empty() {
        let lines: Vec<String> = started.iter().map(downloads::download_line).collect();
        let data: Vec<Value> = started.iter().map(downloads::download_json).collect();
        let text = format!("ok open {url}: {}", lines.join("; "));
        return Ok((
            text,
            json!({ "action": "open", "opened": "download", "downloads": data }),
        ));
    }
    match navigated {
        Ok(()) => {}
        Err(error) if error.kind() == Some("cdp_load_timeout") => {}
        Err(error) => return Err(error),
    }
    let info = PageInfo::read(session.connection(), &page.target_id, deadline)?;
    let text = format!(
        "ok open {} {}",
        short_url(from, &info.url),
        quote_text(&info.title)
    );
    let data = json!({ "action": "open", "url": info.url, "title": info.title });
    Ok((text, data))
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
        text.push_str(if checked {
            " [checked]"
        } else {
            " [unchecked]"
        });
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
    let mut text = format!(
        "ok find {} ({} matches)",
        quote_text(&needle),
        matches.len()
    );
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

/// `js "EXPRESSION"`, or `js` and the expression as written: its value as
/// compact JSON.
pub(crate) fn js_step(
    session: &mut Session,
    step: &BatchStep,
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    let expression = match step.args() {
        [only] if only.quoted => only.text.clone(),
        _ => step
            .source
            .split_once(char::is_whitespace)
            .map_or_else(String::new, |(_, rest)| rest.trim().to_owned()),
    };
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
    let report = current_page_view(session)?;
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
