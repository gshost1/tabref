use serde_json::json;
use tabref_core::AppError;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::find_matches::find_matches;

const MAX_MATCHES: usize = 25;

/// Searches a new snapshot of the whole page, text included, and prints
/// each match with the ref to act on.
pub(crate) fn run(
    session: &mut Session,
    text: &str,
    role: Option<&str>,
) -> Result<Outcome, AppError> {
    let captured = session.capture()?;
    let matches = find_matches(&captured.tree, text, role);
    let shown: Vec<&String> = matches.iter().take(MAX_MATCHES).collect();
    let mut out = if matches.is_empty() {
        format!("no match for {text:?} on {}", captured.info.url)
    } else {
        shown
            .iter()
            .map(|line| line.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    };
    if matches.len() > MAX_MATCHES {
        out.push_str(&format!(
            "\n... {} more; narrow the text or pass --role",
            matches.len() - MAX_MATCHES
        ));
    }
    Ok(Outcome::new(
        out,
        json!({
            "snapshot_id": captured.snapshot_id,
            "url": captured.info.url,
            "count": matches.len(),
            "matches": shown,
        }),
    ))
}
