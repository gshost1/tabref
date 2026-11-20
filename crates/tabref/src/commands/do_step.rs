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
use super::{check, click, fill, focus, hover, press, scroll, select, type_text};
use crate::session::Session;
use crate::view::node_line::quote_text;

const VERBS: &str = "open, click, double-click, fill, type, select, check, uncheck, press, focus, hover, scroll, wait, expect, read, find, js, snap";
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
        "click" | "double-click" => {
            let count = if step.verb() == "click" { 1 } else { 2 };
            let found = target(session, args, LocateIntent::Click, deadline)?;
            click::act(session, found, count, deadline)?
        }
        "focus" => {
            let found =
                focus::field_then_control(|intent| target(session, args, intent, deadline))?;
            focus::act(session, found, deadline)?
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
        "fill" => {
            let (aim, value) = split_value(step)?;
            let found = target(session, aim, LocateIntent::Fill, deadline)?;
            fill::act(session, found, &value, deadline)?
        }
        "type" => {
            let (aim, value) = split_value(step)?;
            let found = target(session, aim, LocateIntent::Fill, deadline)?;
            type_text::act(session, found, &value, deadline)?
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
        (Some(effects), false, "click" | "double-click" | "press") => session
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

/// Splits a fill or select step into its target and its value. When the
/// target ends in a quoted name or is a ref (`fill "Title" 401 bad gateway`,
/// `fill e4 Ada Lovelace`), every word after it is the value, so the value
/// needs no quotes. Otherwise the last word is the value, and a target of
/// several bare words is refused with the advice to quote it. A value that
/// would start with a bare `in`, `after`, `before` or `nth` is refused too:
/// that is a scope the step left without a value.
pub(crate) fn split_value(step: &BatchStep) -> Result<(&[Word], String), AppError> {
    let args = step.args();
    let split = (1..args.len())
        .rev()
        .find(|&end| ends_target(&args[..end]) && TargetSpec::from_words(&args[..end]).is_ok())
        .unwrap_or(args.len().saturating_sub(1));
    let (aim, value) = args.split_at(split);
    if aim.is_empty() || value.is_empty() {
        return Err(AppError::invalid_args(format!(
            "{} needs a target and a value, as in: {} \"Price\" 62.00",
            step.verb(),
            step.verb()
        )));
    }
    if let Some(first) = value.first().filter(|word| !word.quoted)
        && SCOPES.contains(&first.text.to_ascii_lowercase().as_str())
    {
        return Err(AppError::invalid_args(format!(
            "{} has no value after its target; quote the value if it starts with '{}'",
            step.verb(),
            first.text
        )));
    }
    let words: Vec<&str> = value.iter().map(|word| word.text.as_str()).collect();
    Ok((aim, words.join(" ")))
}

const SCOPES: &[&str] = &["in", "after", "before", "nth"];

fn ends_target(aim: &[Word]) -> bool {
    let Some(last) = aim.last() else {
        return false;
    };
    let after_nth = aim.len() >= 2
        && aim[aim.len() - 2].text.eq_ignore_ascii_case("nth")
        && last.text.chars().all(|c| c.is_ascii_digit());
    last.quoted || after_nth || (aim.len() == 1 && is_ref(&last.text))
}

fn is_ref(text: &str) -> bool {
    let short = text.strip_prefix('@').unwrap_or(text);
    text.starts_with("@s")
        || short
            .strip_prefix('e')
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
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
        let first = outcome.text.lines().next().unwrap_or_default();
        return Ok(StepDone {
            text: format!("ok {first}"),
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
