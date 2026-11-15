use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{LocateIntent, Word, scroll_into_view};

use super::act_target::{ActTarget, resolve_target};
use super::action_report::step_report;
use super::batch_step::BatchStep;
use super::do_read_steps::{expect_step, find_step, js_step, open_step, read_step, snap_step};
use super::page_url::short_url;
use super::step_done::StepDone;
use super::target_spec::TargetSpec;
use super::{check, click, fill, hover, press, scroll, select};
use crate::session::Session;
use crate::view::node_line::quote_text;

const VERBS: &str = "open, click, fill, select, check, uncheck, press, hover, scroll, wait, expect, read, find, js, snap";
const DIRECTIONS: &[&str] = &["down", "up", "left", "right"];

/// Runs one step of a batch within `deadline`. The last step reports what
/// it set off in full, as the single command would.
pub(crate) fn run_step(
    session: &mut Session,
    step: &BatchStep,
    last: bool,
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    let args = step.args();
    let acted = match step.verb().as_str() {
        "open" | "goto" => return open_step(session, args, last, deadline),
        "wait" | "expect" => return expect_step(session, step, deadline),
        "read" => return read_step(session, args, deadline),
        "find" => return find_step(session, args),
        "js" => return js_step(session, step, deadline),
        "snap" => return snap_step(session),
        "scroll" => return scroll_step(session, args, deadline),
        "click" => {
            let found = target(session, args, LocateIntent::Click, deadline)?;
            click::act(session, found, deadline)?
        }
        "hover" => {
            let found = target(session, args, LocateIntent::Hover, deadline)?;
            hover::act(session, found, deadline)?
        }
        "check" | "uncheck" => {
            let wanted = step.verb() == "check";
            let found = target(session, args, LocateIntent::Check, deadline)?;
            check::act(session, found, wanted, deadline)?
        }
        "fill" | "type" => {
            let (aim, value) = split_value(step)?;
            let found = target(session, aim, LocateIntent::Fill, deadline)?;
            fill::act(session, found, &value, deadline)?
        }
        "select" => {
            let (aim, value) = split_value(step)?;
            let found = target(session, aim, LocateIntent::Select, deadline)?;
            select::act(session, found, &value, deadline)?
        }
        "press" => {
            let (key, aim) = args
                .split_first()
                .ok_or_else(|| AppError::invalid_args("press needs a key, as in: press Enter"))?;
            let aim = aim.strip_prefix(&[Word::bare("in")]).unwrap_or(aim);
            let found = match aim {
                [] => None,
                words => Some(target(session, words, LocateIntent::Click, deadline)?),
            };
            press::act(session, &key.text, found, deadline)?
        }
        other => {
            return Err(AppError::invalid_args(format!(
                "'{other}' is not a step; steps are {VERBS}"
            )));
        }
    };
    let late = match (&acted.effects, last, step.verb().as_str()) {
        (Some(effects), false, "click" | "press") => session
            .follow_late(effects, deadline)?
            .map(|page| (effects.url_before.clone(), page)),
        _ => None,
    };
    let (mut text, data) = step_report(session, acted, last)?;
    if let Some((before, page)) = late {
        text.push_str(&format!(
            " -> {} {} (after the step settled)",
            short_url(&before, &page.url),
            quote_text(&page.title)
        ));
    }
    Ok(StepDone { text, data })
}

fn target(
    session: &mut Session,
    words: &[Word],
    intent: LocateIntent,
    deadline: Deadline,
) -> Result<ActTarget, AppError> {
    if words.is_empty() {
        return Err(AppError::invalid_args(
            "the step names no target; give a ref or a name",
        ));
    }
    resolve_target(session, &TargetSpec::from_words(words)?, intent, deadline)
}

/// Splits a fill or select step into its target and its value: the target
/// is the longest run of leading words that reads as one, and every word
/// after it is the value, so `fill "Title" 401 bad gateway` needs no quotes
/// around the value.
pub(crate) fn split_value(step: &BatchStep) -> Result<(&[Word], String), AppError> {
    let args = step.args();
    let split = (1..args.len())
        .rev()
        .find(|&end| TargetSpec::from_words(&args[..end]).is_ok());
    match split {
        Some(end) => {
            let value: Vec<&str> = args[end..].iter().map(|word| word.text.as_str()).collect();
            Ok((&args[..end], value.join(" ")))
        }
        None => Err(AppError::invalid_args(format!(
            "{} needs a target and a value, as in: {} \"Price\" 62.00",
            step.verb(),
            step.verb()
        ))),
    }
}

fn scroll_step(
    session: &mut Session,
    args: &[Word],
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    let direction = match args.first() {
        None => Some("down"),
        Some(word) if !word.quoted => DIRECTIONS
            .iter()
            .copied()
            .find(|direction| word.text.eq_ignore_ascii_case(direction)),
        Some(_) => None,
    };
    if let Some(direction) = direction {
        let container = match args.get(1..) {
            Some(rest) if !rest.is_empty() => {
                let rest = rest.strip_prefix(&[Word::bare("in")]).unwrap_or(rest);
                Some(TargetSpec::from_words(rest)?)
            }
            _ => None,
        };
        let outcome = scroll::scroll(session, direction, container.as_ref(), None, deadline)?;
        return Ok(StepDone {
            text: format!("ok {}", outcome.text),
            data: outcome.data,
        });
    }
    let found = target(session, args, LocateIntent::Click, deadline)?;
    scroll_into_view(session.connection(), &found.session, &found.token, deadline)?;
    Ok(StepDone {
        text: format!("ok scroll {} into view", found.label),
        data: json!({ "action": "scroll", "target": found.label }),
    })
}

#[cfg(test)]
#[path = "do_step_tests.rs"]
mod tests;
