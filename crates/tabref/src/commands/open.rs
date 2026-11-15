use std::path::Path;
use std::time::Duration;

use tabref_cdp::{PageSession, navigate_by};
use tabref_core::{AppError, Deadline};
use tabref_page::{DownloadRecord, PageInfo};

use super::action_report::set;
use super::download_folder::choose_folder;
use super::downloads;
use super::new_tab::{in_new_tab, new_tab_address, open_tab};
use super::page_url::resolve_url;
use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::view::snapshot_view::snapshot_view;
use crate::view::view_request::ViewRequest;
use crate::view::view_scope::ViewScope;

const SCHEMES: &[&str] = &["about:", "data:", "file:", "chrome:", "javascript:"];
const SNAPSHOT_RESERVE_MS: u64 = 1_000;

/// Navigates the current tab to `url`, or a new tab that becomes the
/// current one when `new_tab` is set, and prints the loaded page's view,
/// so reading it costs no second command. A relative address resolves
/// against the current page. A page that has not fired `load` after three
/// quarters of the deadline is snapshotted as it stands, with a note. The
/// browser may take until one second before the deadline to start the new
/// document, as a browser that has just started can hold its first page
/// load for seconds. An address that downloads reports the saved file and
/// leaves the page as it was. `download_dir` chooses where this browser's
/// downloads land from now on.
pub(crate) fn run(
    session: &mut Session,
    url: &str,
    (new_tab, download_dir): (bool, Option<&Path>),
) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    if let Some(dir) = download_dir {
        choose_folder(session, dir)?;
    }
    if new_tab {
        let url = new_tab_address(session, url)?;
        let (index, page) = open_tab(session, deadline)?;
        let mut report =
            load(session, &page, &url, deadline).map_err(|error| in_new_tab(error, index, &url))?;
        report.text = format!("new tab [{index}]\n{}", report.text);
        set(&mut report.data, "new_tab_index", serde_json::json!(index));
        return Ok(report);
    }
    let page = session.page()?;
    let url = normalize_url(url);
    let url = if is_relative(&url) {
        let current = PageInfo::read(session.connection(), &page.target_id, deadline)?.url;
        resolve_url(&current, &url)
    } else {
        url
    };
    load(session, &page, &url, deadline).map_err(|error| {
        if error.kind() == Some("cdp_navigation_pending") {
            open_again(error)
        } else {
            error
        }
    })
}

fn load(
    session: &mut Session,
    page: &PageSession,
    url: &str,
    deadline: Deadline,
) -> Result<Outcome, AppError> {
    let remaining = deadline.remaining_ms();
    let load = remaining * 3 / 4;
    let commit = remaining.saturating_sub(SNAPSHOT_RESERVE_MS).max(load);
    let _ = downloads::enable(session, deadline);
    let navigated = navigate_by(
        session.connection(),
        page,
        url,
        deadline.capped(Duration::from_millis(commit)),
        deadline.capped(Duration::from_millis(load)),
    );
    let started = downloads::after_action(session, deadline, downloads::grace_after(&navigated));
    if !started.is_empty() {
        return Ok(downloaded(&started));
    }
    let unfinished = match navigated {
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

/// An `open` whose address downloaded: one line per download, the page
/// left as it was.
fn downloaded(started: &[DownloadRecord]) -> Outcome {
    let lines: Vec<String> = started.iter().map(downloads::download_line).collect();
    let data: Vec<serde_json::Value> = started.iter().map(downloads::download_json).collect();
    Outcome::new(
        lines.join("\n"),
        serde_json::json!({ "opened": "download", "downloads": data }),
    )
}

/// A navigation the browser did not start in time, for `open` alone:
/// opening the same address again only asks for it again, so it may be
/// repeated. Other callers keep the neutral error, as repeating them could
/// repeat the actions before the navigation.
fn open_again(error: AppError) -> AppError {
    let mut details = error
        .details
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    details["retryable"] = serde_json::json!(true);
    error
        .with_suggestion(
            "A slow server, or a browser that has just started and is still loading its \
             profile, holds the page back; run the same open again, or pass a longer --timeout",
        )
        .with_details(details)
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
