use tabref_core::{AppError, Deadline};
use tabref_page::Word;

use super::scroll_until::scroll_loop;
use crate::commands::step_done::StepDone;
use crate::commands::target_spec::TargetSpec;
use crate::session::Session;
use crate::view::node_line::quote_text;

pub(super) fn quote_until_value(text: &str) -> AppError {
    AppError::invalid_args(format!(
        "extra value words must be one argument; quote the value: tabref scroll down --until {}",
        quote_text(text)
    ))
}

pub(crate) struct BatchScroll {
    pub direction: String,
    pub container: Option<TargetSpec>,
    pub until: Option<String>,
    pub to_end: bool,
}

fn direction(word: &Word) -> bool {
    !word.quoted
        && ["down", "up", "left", "right"]
            .iter()
            .any(|value| word.text.eq_ignore_ascii_case(value))
}

fn boundary(word: &Word) -> bool {
    word.is_keyword("in") || direction(word) || (!word.quoted && word.text.starts_with("--"))
}

pub(super) fn parse_cli_scroll(
    heading: &str,
    target: Option<&str>,
    values: &[String],
) -> Result<BatchScroll, AppError> {
    let (text, tail) = values.split_first().ok_or_else(|| quote_until_value(""))?;
    let mut words = vec![Word::bare("--until"), Word::quoted(text)];
    let tail: Vec<Word> = tail.iter().map(|word| Word::bare(word)).collect();
    let needs_heading = !tail.iter().any(direction);
    words.extend(tail);
    if needs_heading {
        words.push(Word::bare(heading));
    }
    let mut parsed = parse_batch_scroll(&words)?;
    if let Some(target) = target {
        if parsed.container.is_some() {
            return Err(AppError::invalid_args("give one scroll container"));
        }
        parsed.container = Some(TargetSpec::from_arg(target)?);
    }
    Ok(parsed)
}

pub(crate) fn parse_batch_scroll(args: &[Word]) -> Result<BatchScroll, AppError> {
    let mut until = None;
    let mut to_end = false;
    let mut positional = Vec::new();
    let mut words = args.iter().peekable();
    while let Some(word) = words.next() {
        match word.text.as_str() {
            "--until" if !word.quoted => {
                if until.is_some() || to_end {
                    return Err(AppError::invalid_args(
                        "use --until TEXT or --to-end once, not both",
                    ));
                }
                let text = words
                    .next()
                    .filter(|word| word.quoted || !word.text.starts_with("--"))
                    .ok_or_else(|| {
                        AppError::invalid_args("--until needs TEXT; quote text containing spaces")
                    })?;
                if words.peek().is_some_and(|word| !boundary(word)) {
                    let mut text = text.text.clone();
                    while let Some(word) = words.next_if(|word| !boundary(word)) {
                        text.push(' ');
                        text.push_str(&word.text);
                    }
                    return Err(quote_until_value(&text));
                }
                until = Some(text.text.clone());
            }
            "--to-end" if !word.quoted => {
                if until.is_some() || to_end {
                    return Err(AppError::invalid_args(
                        "use --until TEXT or --to-end once, not both",
                    ));
                }
                to_end = true;
            }
            text if !word.quoted && text.starts_with("--") => {
                return Err(AppError::invalid_args(format!(
                    "unknown scroll option '{text}'"
                )));
            }
            _ => positional.push(word.clone()),
        }
    }
    let heading = positional
        .iter()
        .position(direction)
        .map(|index| positional.remove(index).text.to_ascii_lowercase())
        .unwrap_or_else(|| "down".to_owned());
    let rest = positional.as_slice();
    if rest.first().is_some_and(|word| !word.is_keyword("in"))
        && args.iter().all(|word| !direction(word))
    {
        return Err(AppError::invalid_args(
            "--until and --to-end require a scroll direction, not a REF",
        ));
    }
    let container = if rest.is_empty() {
        None
    } else {
        let rest = rest.strip_prefix(&[Word::bare("in")]).unwrap_or(rest);
        Some(TargetSpec::from_words(rest)?)
    };
    Ok(BatchScroll {
        direction: heading,
        container,
        until,
        to_end,
    })
}

pub(crate) fn run_batch_loop(
    session: &mut Session,
    args: &[Word],
    deadline: Deadline,
) -> Result<StepDone, AppError> {
    let parsed = parse_batch_scroll(args)?;
    let outcome = scroll_loop(
        session,
        &parsed.direction,
        parsed.container.as_ref(),
        None,
        (parsed.until.as_deref(), parsed.to_end, deadline),
    )?;
    let mut lines = outcome.text.lines();
    let mut text = format!("ok {}", lines.next().unwrap_or_default());
    for line in lines {
        text.push_str("\n  ");
        text.push_str(line);
    }
    Ok(StepDone {
        text,
        data: outcome.data,
    })
}
