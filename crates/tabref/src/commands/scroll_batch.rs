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
                if words
                    .peek()
                    .is_some_and(|word| word.quoted || !word.text.starts_with("--"))
                {
                    let mut text = text.text.clone();
                    while let Some(word) =
                        words.next_if(|word| word.quoted || !word.text.starts_with("--"))
                    {
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
    let (direction, rest) = match positional.split_first() {
        None => ("down".to_owned(), &[][..]),
        Some((word, rest))
            if !word.quoted
                && ["down", "up", "left", "right"]
                    .iter()
                    .any(|direction| word.text.eq_ignore_ascii_case(direction)) =>
        {
            (word.text.to_ascii_lowercase(), rest)
        }
        _ => {
            return Err(AppError::invalid_args(
                "--until and --to-end require a scroll direction, not a REF",
            ));
        }
    };
    let container = if rest.is_empty() {
        None
    } else {
        let rest = rest.strip_prefix(&[Word::bare("in")]).unwrap_or(rest);
        Some(TargetSpec::from_words(rest)?)
    };
    Ok(BatchScroll {
        direction,
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
