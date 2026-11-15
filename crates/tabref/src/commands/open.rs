use std::path::Path;
use std::time::Duration;

use tabref_cdp::navigate_by;
use tabref_core::AppError;
use tabref_page::PageInfo;

use super::action_report::set;
use super::page_url::resolve_url;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::snapshot_view::snapshot_view;
use crate::view::view_request::ViewRequest;
use crate::view::view_scope::ViewScope;

const SCHEMES: &[&str] = &["about:", "data:", "file:", "chrome:", "javascript:"];
const SNAPSHOT_RESERVE_MS: u64 = 1_000;

/// Navigates the current tab to `url` and prints the loaded page's view,
/// so reading it costs no second command. A page that has not fired `load` after three quarters of the
/// deadline is snapshotted as it stands, with a note. The browser may take
/// until one second before the deadline to start the new document, as a
/// browser that has just started can hold its first page load for seconds.
pub(crate) fn run(session: &mut Session, url: &str) -> Result<Outcome, AppError> {
    let page = session.page()?;
    let deadline = session.deadline();
    let url = normalize_url(url);
    let url = if is_relative(&url) {
        let current = PageInfo::read(session.connection(), &page.target_id, deadline)?.url;
        resolve_url(&current, &url)
    } else {
        url
    };
    let remaining = deadline.remaining_ms();
    let load = remaining * 3 / 4;
    let commit = remaining.saturating_sub(SNAPSHOT_RESERVE_MS).max(load);
    let unfinished = match navigate_by(
        session.connection(),
        &page,
        &url,
        deadline.capped(Duration::from_millis(commit)),
        deadline.capped(Duration::from_millis(load)),
    ) {
        Ok(()) => false,
        Err(error) if error.kind() == Some("cdp_load_timeout") => true,
        Err(error) => return Err(error),
    };
    let captured = session.capture()?;
    let options = ViewRequest::default().snap_options(&captured.info.url)?;
    let mut report = snapshot_view(session, &captured, &options, &ViewScope::Page)?;
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

/// `example.com` becomes `https://example.com`, a loopback host such as
/// `localhost:8000` gets `http://`, an existing local file becomes a
/// `file://` URL, and anything with a scheme stays as it is. A path, query
/// or fragment of the current page (`/admin/`, `?page=2`, `#top`, `./a`)
/// stays relative for the caller to resolve against that page.
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
    if is_relative(url) {
        return url.to_owned();
    }
    let host = url.split(['/', '?', '#']).next().unwrap_or_default();
    let bare_host = host.rsplit_once(':').map_or(host, |(name, port)| {
        if port.chars().all(|c| c.is_ascii_digit()) {
            name
        } else {
            host
        }
    });
    if matches!(bare_host, "localhost" | "127.0.0.1" | "[::1]") {
        return format!("http://{url}");
    }
    format!("https://{url}")
}

pub(crate) fn is_relative(url: &str) -> bool {
    ["/", "?", "#", "./"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
