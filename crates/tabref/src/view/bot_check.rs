use serde_json::json;
use tabref_page::{DomNode, DomTree, PageInfo};

use crate::output::outcome::Outcome;

const NOTE: &str = "note: bot check on this page";

/// Recognizes a challenge in the top-level DOM, using the document's title
/// and markup or the visible owner of a provider's challenge iframe.
pub(crate) fn bot_check_view(info: &PageInfo, document: Option<&DomTree>) -> Option<Outcome> {
    let document = document?;
    let cloudflare_title = info
        .title
        .trim()
        .trim_end_matches(['.', '…'])
        .trim_end()
        .eq_ignore_ascii_case("just a moment");
    let blocked = document
        .nodes
        .iter()
        .any(|node| (cloudflare_title && cloudflare_marker(node)) || visible_captcha(node));
    blocked.then(|| {
        Outcome::new(
            NOTE,
            json!({ "bot_check": true, "url": info.url, "title": info.title, "lines": [NOTE] }),
        )
    })
}

fn cloudflare_marker(node: &DomNode) -> bool {
    matches!(
        node.attr("id"),
        Some("challenge-stage" | "challenge-form" | "cf-challenge-running" | "challenge-body-text")
    ) || (matches!(node.tag(), "SCRIPT" | "IFRAME")
        && node
            .attr("src")
            .is_some_and(|src| src.contains("/cdn-cgi/challenge-platform/")))
}

fn visible_captcha(node: &DomNode) -> bool {
    node.tag() == "IFRAME"
        && node.layout.as_ref().is_some_and(|layout| {
            let [x, y, width, height] = layout.bounds;
            layout.visible && !layout.is_tiny() && x + width > 0.0 && y + height > 0.0
        })
        && node.attr("src").is_some_and(challenge_url)
}

/// Matches challenge frames, including reCAPTCHA Enterprise, while leaving
/// checkbox widgets, badges and ordinary embeds on the page.
fn challenge_url(src: &str) -> bool {
    let src = src.trim();
    let Some((host, rest)) = src
        .strip_prefix("https://")
        .or_else(|| src.strip_prefix("//"))
        .and_then(|url| url.split_once('/'))
    else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let (path, params) = rest.split_once(['?', '#']).unwrap_or((rest, ""));
    if matches!(
        host.as_str(),
        "google.com" | "www.google.com" | "recaptcha.net" | "www.recaptcha.net"
    ) {
        return matches!(
            path,
            "recaptcha/api2/bframe" | "recaptcha/enterprise/bframe"
        );
    }
    (host == "hcaptcha.com" || host.ends_with(".hcaptcha.com"))
        && path.starts_with("captcha/")
        && params
            .split(['&', '#'])
            .any(|part| part == "frame=challenge")
}
