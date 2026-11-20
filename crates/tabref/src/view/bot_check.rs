use tabref_page::{DomNode, DomTree, PageInfo};

use crate::output::outcome::Outcome;

const NOTE: &str = "note: bot check on this page";

/// Recognizes a challenge in the top-level DOM, using the document's title
/// and markup or the visible owner of a provider's challenge iframe.
pub(crate) fn bot_check_view(info: &PageInfo, document: Option<&DomTree>, report: &mut Outcome) {
    let Some(provider) = document.and_then(|document| provider(info, document)) else {
        return;
    };
    report.text.insert_str(0, &format!("{NOTE} ({provider})\n"));
    report.data["bot_check"] = true.into();
    report.data["bot_check_provider"] = provider.into();
}

fn provider(info: &PageInfo, document: &DomTree) -> Option<&'static str> {
    let cloudflare_title = info
        .title
        .trim()
        .trim_end_matches(['.', '…'])
        .trim_end()
        .eq_ignore_ascii_case("just a moment");
    if cloudflare_title && document.nodes.iter().any(cloudflare_marker) {
        return Some("Cloudflare");
    }
    document.nodes.iter().find_map(visible_captcha)
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
