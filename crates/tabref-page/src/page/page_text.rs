use serde_json::Value;
use tabref_core::{AppError, Deadline};

use super::evaluate::evaluate;
use crate::{CdpConnection, PageSession};

/// The rendered text of the page's main document, `document.body.innerText`,
/// with runs of blank lines squeezed to one and at most `limit` characters.
/// Returns the text and whether it was cut at the limit.
pub fn page_text(
    connection: &mut CdpConnection,
    session: &PageSession,
    limit: usize,
    deadline: Deadline,
) -> Result<(String, bool), AppError> {
    let value = evaluate(
        connection,
        session,
        "document.body ? document.body.innerText : document.documentElement.innerText",
        deadline,
    )?;
    let raw = match value {
        Value::String(text) => text,
        _ => String::new(),
    };
    let text = squeeze_blank_lines(&raw);
    let total = text.chars().count();
    if total <= limit {
        return Ok((text, false));
    }
    Ok((text.chars().take(limit).collect(), true))
}

pub(crate) fn squeeze_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

#[cfg(test)]
#[path = "page_text_tests.rs"]
mod tests;
