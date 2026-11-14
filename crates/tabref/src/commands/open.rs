use std::path::Path;
use std::time::Duration;

use tabref_cdp::navigate;
use tabref_core::AppError;

use super::action_report::set;
use crate::output::outcome::Outcome;
use crate::output::page_report::page_report;
use crate::session::Session;
use crate::view::view_mode::ViewMode;

const SCHEMES: &[&str] = &["about:", "data:", "file:", "chrome:", "javascript:"];

/// Navigates the current tab to `url` and prints a snapshot of the loaded
/// page. A page that has not fired `load` after three quarters of the
/// deadline is snapshotted as it stands, with a note.
pub(crate) fn run(session: &mut Session, url: &str) -> Result<Outcome, AppError> {
    let url = normalize_url(url);
    let page = session.page()?;
    let deadline = session.deadline();
    let navigation = deadline.capped(Duration::from_millis(deadline.remaining_ms() * 3 / 4));
    let unfinished = match navigate(session.connection(), &page, &url, navigation) {
        Ok(()) => false,
        Err(error) if error.kind() == Some("cdp_load_timeout") => true,
        Err(error) => return Err(error),
    };
    let captured = session.capture()?;
    let mut report = page_report(&captured, ViewMode::Interactive);
    if unfinished {
        report
            .push_line("note: the page had not finished loading; snap again if parts are missing");
    }
    set(
        &mut report.data,
        "load_finished",
        serde_json::json!(!unfinished),
    );
    Ok(report)
}

/// `example.com` becomes `https://example.com`, an existing local file a
/// `file://` URL, and anything with a scheme stays as it is.
pub(crate) fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.contains("://") || SCHEMES.iter().any(|scheme| url.starts_with(scheme)) {
        return url.to_owned();
    }
    let path = Path::new(url);
    if path.is_file()
        && let Ok(absolute) = std::path::absolute(path)
    {
        let text = absolute.to_string_lossy().replace('\\', "/");
        let separator = if text.starts_with('/') { "" } else { "/" };
        return format!("file://{separator}{text}");
    }
    format!("https://{url}")
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
