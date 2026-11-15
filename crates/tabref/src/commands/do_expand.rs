//! `for` over literal values in a `do` batch:
//!
//! ```text
//! for id price in 4499 62.00 5916 64.00
//!   open /admin/catalog/product/{id}/edit
//!   fill "Price" {price}
//! end
//! ```
//!
//! The values are taken a name's worth at a time and `{name}` is replaced in
//! each step of the body before anything runs, so a loop is only a shorter
//! way to write the steps out. Loops may nest.

use tabref_core::AppError;
use tabref_page::Word;

use super::batch_step::BatchStep;

const MAX_STEPS: usize = 500;

/// `steps` with every `for ... end` written out.
pub(crate) fn expand(steps: &[BatchStep]) -> Result<Vec<BatchStep>, AppError> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < steps.len() {
        let step = &steps[index];
        match step.verb().as_str() {
            "end" => {
                return Err(AppError::invalid_args(format!(
                    "line {}: end without a for",
                    step.line
                )));
            }
            "for" => {
                let close = matching_end(steps, index)?;
                let body = expand(&steps[index + 1..close])?;
                let (names, values) = loop_words(step)?;
                for chunk in values.chunks(names.len()) {
                    for inner in &body {
                        out.push(substitute(inner, &names, chunk));
                        if out.len() > MAX_STEPS {
                            return Err(too_many());
                        }
                    }
                }
                index = close + 1;
            }
            _ => {
                out.push(step.clone());
                index += 1;
            }
        }
        if out.len() > MAX_STEPS {
            return Err(too_many());
        }
    }
    Ok(out)
}

fn too_many() -> AppError {
    AppError::invalid_args(format!(
        "the batch runs more than {MAX_STEPS} steps; split it"
    ))
}

fn matching_end(steps: &[BatchStep], start: usize) -> Result<usize, AppError> {
    let mut depth = 0;
    for (offset, step) in steps[start..].iter().enumerate() {
        match step.verb().as_str() {
            "for" => depth += 1,
            "end" => {
                depth -= 1;
                if depth == 0 {
                    return Ok(start + offset);
                }
            }
            _ => {}
        }
    }
    Err(AppError::invalid_args(format!(
        "line {}: for without an end",
        steps[start].line
    )))
}

fn loop_words(step: &BatchStep) -> Result<(Vec<String>, Vec<String>), AppError> {
    let args = step.args();
    let split = args
        .iter()
        .position(|word| word.is_keyword("in"))
        .ok_or_else(|| usage(step))?;
    let names: Vec<String> = args[..split].iter().map(|word| word.text.clone()).collect();
    let values: Vec<String> = args[split + 1..]
        .iter()
        .map(|word| word.text.clone())
        .collect();
    if names.is_empty() || values.is_empty() || values.len() % names.len() != 0 {
        return Err(usage(step));
    }
    Ok((names, values))
}

fn usage(step: &BatchStep) -> AppError {
    AppError::invalid_args(format!(
        "line {}: write for NAME [NAME...] in VALUES..., with one value per name each round",
        step.line
    ))
}

/// The step with `{name}` replaced by its value in one pass over the
/// written text, so a value that itself reads `{other}` stays as it is. A
/// word that took a value is quoted, so a value never turns into a role
/// word, a keyword or a ref.
fn substitute(step: &BatchStep, names: &[String], values: &[String]) -> BatchStep {
    let fill = |text: &str| {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        let mut filled = false;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let value = after.find('}').and_then(|close| {
                let name = &after[..close];
                let index = names.iter().position(|known| known == name)?;
                Some((close, values.get(index)?))
            });
            match value {
                Some((close, value)) => {
                    out.push_str(value);
                    rest = &after[close + 1..];
                    filled = true;
                }
                None => {
                    out.push('{');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        (out, filled)
    };
    BatchStep {
        words: step
            .words
            .iter()
            .map(|word| {
                let (text, filled) = fill(&word.text);
                Word {
                    text,
                    quoted: word.quoted || filled,
                }
            })
            .collect(),
        source: fill(&step.source).0,
        line: step.line,
    }
}

#[cfg(test)]
#[path = "do_expand_tests.rs"]
mod tests;
