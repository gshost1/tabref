//! Splits the text of a `do` batch into steps and words without a shell: a
//! newline or a `;` outside quotes ends a step, `#` at the start of a step
//! makes the rest of its line a comment, double quotes take `\"`, `\\`,
//! `\n` and `\t`, and single quotes take their text as it is. The text
//! reaches tabref through a quoted heredoc or one quoted argument, so the
//! shell never sees a loop or a variable.

use tabref_core::AppError;
use tabref_page::Word;

use super::batch_step::BatchStep;

/// The steps of `text`, empty and comment-only steps left out.
pub(crate) fn split_steps(text: &str) -> Result<Vec<BatchStep>, AppError> {
    scan(text, true)
}

/// The words of one target or step written as one argument, where a `;`
/// or a line break is part of the text.
pub(crate) fn split_words(text: &str) -> Result<Vec<Word>, AppError> {
    Ok(scan(text, false)?
        .into_iter()
        .flat_map(|step| step.words)
        .collect())
}

fn scan(text: &str, breaks: bool) -> Result<Vec<BatchStep>, AppError> {
    let mut steps = Vec::new();
    let mut scanner = Scanner::default();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if breaks && scanner.quote.is_none() && matches!(c, '\n' | ';') {
            scanner.end_step(&mut steps);
            if c == '\n' {
                scanner.line += 1;
            }
            continue;
        }
        if breaks && scanner.quote.is_none() && c == '#' && scanner.at_step_start() {
            while chars.peek().is_some_and(|next| *next != '\n') {
                chars.next();
            }
            continue;
        }
        scanner.source.push(c);
        if c == '\n' {
            scanner.line += 1;
        }
        match scanner.quote {
            Some(quote) if c == quote => {
                scanner.quote = None;
                scanner.close_quoted();
            }
            Some('"') if c == '\\' => {
                let escaped = chars.next();
                if let Some(next) = escaped {
                    scanner.source.push(next);
                }
                scanner.word.push(match escaped {
                    Some('n') => '\n',
                    Some('t') => '\t',
                    Some(other) => other,
                    None => '\\',
                });
            }
            Some(_) => scanner.word.push(c),
            None if c.is_whitespace() => scanner.end_word(),
            None if matches!(c, '"' | '\'') && !scanner.in_word => {
                scanner.quote = Some(c);
                scanner.in_word = true;
            }
            None => {
                scanner.word.push(c);
                scanner.in_word = true;
            }
        }
    }
    if let Some(quote) = scanner.quote {
        return Err(AppError::invalid_args(format!(
            "line {}: a {quote} quote is never closed",
            scanner.step_line + 1
        )));
    }
    scanner.end_step(&mut steps);
    Ok(steps)
}

#[derive(Default)]
struct Scanner {
    words: Vec<Word>,
    word: String,
    in_word: bool,
    quote: Option<char>,
    source: String,
    line: usize,
    step_line: usize,
}

impl Scanner {
    fn at_step_start(&self) -> bool {
        self.words.is_empty() && !self.in_word
    }

    fn close_quoted(&mut self) {
        self.words.push(Word::quoted(&self.word));
        self.word.clear();
        self.in_word = false;
    }

    fn end_word(&mut self) {
        if self.in_word {
            self.words.push(Word::bare(&self.word));
            self.word.clear();
            self.in_word = false;
        }
        if self.words.is_empty() {
            self.source.clear();
            self.step_line = self.line;
        }
    }

    fn end_step(&mut self, steps: &mut Vec<BatchStep>) {
        self.end_word();
        if !self.words.is_empty() {
            steps.push(BatchStep {
                words: std::mem::take(&mut self.words),
                source: self.source.trim().to_owned(),
                line: self.step_line + 1,
            });
        }
        self.source.clear();
        self.step_line = self.line + 1;
    }
}

#[cfg(test)]
#[path = "step_words_tests.rs"]
mod tests;
