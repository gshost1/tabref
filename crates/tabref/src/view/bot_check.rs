use std::time::Duration;

use serde_json::Value;
use tabref_page::{DomNode, DomTree, PageInfo, capture_dom};

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::captured_snapshot::CapturedSnapshot;

const NOTE: &str = "note: bot check on this page";
const ACTION_CHECK_WAIT: Duration = Duration::from_millis(100);

pub(crate) struct BotCheck(&'static str);

impl BotCheck {
    pub(crate) fn note(&self) -> String {
        format!("{NOTE} ({})", self.0)
    }

    pub(crate) fn add_data(&self, data: &mut Value) {
        data["bot_check"] = true.into();
        data["bot_check_provider"] = self.0.into();
    }
}

/// Recognizes a challenge in the top-level DOM, using the document's title
/// and markup or the visible owner of a provider's challenge iframe.
pub(crate) fn bot_check_view(info: &PageInfo, document: Option<&DomTree>, report: &mut Outcome) {
    let Some(check) = document.and_then(|document| detect(&info.title, document)) else {
        return;
    };
    report.text.insert_str(0, &format!("{}\n", check.note()));
    check.add_data(&mut report.data);
}

/// Reuses the capture taken for new controls when there is one. A short
/// DOM-only probe covers actions and batch steps that allocate no refs.
pub(crate) fn action_bot_check(
    session: &mut Session,
    title: &str,
    captured: Option<&CapturedSnapshot>,
) -> Option<BotCheck> {
    let page = session.page().ok()?;
    if let Some(captured) = captured
        && let Some(document) = captured
            .dom(&page.target_id)
            .and_then(|trees| trees.first())
    {
        return detect(&captured.info.title, document);
    }
    let deadline = session.deadline().capped(ACTION_CHECK_WAIT);
    let trees = capture_dom(session.connection(), &page, deadline).ok()?;
    detect(title, trees.first()?)
}

fn detect(title: &str, document: &DomTree) -> Option<BotCheck> {
    let cloudflare_title = title
        .trim()
        .trim_end_matches(['.', '…'])
        .trim_end()
        .eq_ignore_ascii_case("just a moment");
    if cloudflare_title && document.nodes.iter().any(cloudflare_marker) {
        return Some(BotCheck("Cloudflare"));
    }
    document
        .nodes
        .iter()
        .find_map(visible_captcha)
        .map(BotCheck)
}

fn cloudflare_marker(node: &DomNode) -> bool {
    matches!(
        node.attr("id"),
        Some("challenge-stage" | "challenge-form" | "cf-challenge-running" | "challenge-body-text")
    ) || (matches!(node.tag(), "SCRIPT" | "IFRAME")
        && node.attr("src").is_some_and(|src| {
            src.contains("/cdn-cgi/challenge-platform/") && !src.contains("/scripts/jsd/")
        }))
}

fn visible_captcha(node: &DomNode) -> Option<&'static str> {
    let layout = node.layout.as_ref()?;
    let [x, y, width, height] = layout.bounds;
    if node.tag() == "IFRAME"
        && layout.visible
        && !layout.is_tiny()
        && x + width > 0.0
        && y + height > 0.0
    {
        return challenge_url(node.attr("src")?);
    }
    None
}

/// Matches challenge frames, including reCAPTCHA Enterprise, while leaving
/// checkbox widgets, badges and ordinary embeds on the page.
fn challenge_url(src: &str) -> Option<&'static str> {
    let src = src.trim();
    let (host, rest) = src
        .strip_prefix("https://")
        .or_else(|| src.strip_prefix("//"))
        .and_then(|url| url.split_once('/'))?;
    let host = host.to_ascii_lowercase();
    let (path, params) = rest.split_once(['?', '#']).unwrap_or((rest, ""));
    if matches!(
        host.as_str(),
        "google.com" | "www.google.com" | "recaptcha.net" | "www.recaptcha.net"
    ) {
        return matches!(
            path,
            "recaptcha/api2/bframe" | "recaptcha/enterprise/bframe"
        )
        .then_some("reCAPTCHA");
    }
    ((host == "hcaptcha.com" || host.ends_with(".hcaptcha.com"))
        && path.starts_with("captcha/")
        && params
            .split(['&', '#'])
            .any(|part| part == "frame=challenge"))
    .then_some("hCaptcha")
}

#[cfg(test)]
#[path = "bot_check_tests.rs"]
mod tests;
